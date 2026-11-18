# SPDX-License-Identifier: Apache-2.0
"""Materialize the sealed inputs of a fresh CUDA ONNX Runtime build."""

import hashlib
from pathlib import Path
import shutil
import time
import types


BUILD = Path(__file__).resolve().parent
TARGET = "x86_64-pc-windows-msvc"
CAPI = "onnxruntime/core/session/onnxruntime_c_api.cc"
PUBLIC_API_PATCH = "a4ca229dd0e82751565c3d26dd4cdfb8b258a5050f6d0849c83235d44b75e8d9"
PUBLIC_API_SOURCE = "7acabdbefcd7ee3936305ff150fc2369c3d341b6838e10113435ebab03378ab8"
COLLECTOR_PATCH = "a1a95c9b4401e302d7046159f882fdafa735bf7b32542314f75b333b902870c9"
COLLECTOR_LF_PATCH = "5222d2634878ac5e1c68264b6d521f0ca95df3a47397e3cf167b3ff7de844d58"
COLLECTOR_SOURCE = "db00479a06d0a1e8fadf4cc834ffe5a6da0b745ee2820b06a0821fb1dbeca97b"
MAX_PATCH_BYTES = 1 << 20



def check_deadline(deadline):
    if time.monotonic() >= deadline:
        raise TimeoutError("original CUDA source materialization deadline exhausted")


def pin_bytes(path, raw):
    return {"path": str(path), "bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


def audit_tree(root, guard_paths, deadline):
    # The canonical extractor rejects links and path escapes. Enumerate its
    # actual output in bounded audit batches before hashing the new tree.
    paths = [root, *root.rglob("*")]
    check_deadline(deadline)
    guard_paths(paths, True, deadline)


def audited_cpu(cpu, guard_paths, deadline):
    """Expose guarded output seams without changing the canonical module."""
    def inventory(root, **kwargs):
        audit_tree(root, guard_paths, deadline)
        result = cpu.source_inventory(root, **kwargs)
        check_deadline(deadline)
        return result

    def write_json(path, value):
        guard_paths([path], False, deadline)
        cpu.write_json(path, value)
        guard_paths([path], True, deadline)

    # Canonical source extraction calls its inventory/writer by module global;
    # bind only those output seams for this operation. The frontend owner uses
    # the same checked inventory through its explicit CPU-helper argument.
    extract = cpu.extract_source
    checked_extract = types.FunctionType(
        extract.__code__,
        dict(extract.__globals__, source_inventory=inventory, write_json=write_json),
        extract.__name__, extract.__defaults__, extract.__closure__,
    )
    return types.SimpleNamespace(
        **dict(vars(cpu), source_inventory=inventory, write_json=write_json, extract_source=checked_extract),
    )


def ensure_directory(path, root, guard_paths, deadline):
    if not path.is_relative_to(root):
        raise ValueError("CUDA source directory escapes fresh build root")
    missing = []
    while not path.exists():
        if path == root:
            raise ValueError("fresh CUDA build root has not been checked and created")
        missing.append(path)
        path = path.parent
    for directory in reversed(missing):
        check_deadline(deadline)
        guard_paths([directory], False, deadline)
        directory.mkdir()
        guard_paths([directory], True, deadline)


def mirror_inputs(cpu, mirror, dependencies, archives, guard_paths, deadline):
    for row in dependencies:
        check_deadline(deadline)
        source = cpu.plain(archives[row["name"]], regular=True)
        target = cpu.child(mirror, cpu.mirror_relative(row))
        cpu.verify_file(source, row)
        ensure_directory(target.parent, mirror, guard_paths, deadline)
        guard_paths([target], False, deadline)
        with source.open("rb") as original, target.open("xb") as copy:
            shutil.copyfileobj(original, copy, 1 << 20)
        guard_paths([target], True, deadline)
        cpu.verify_file(target, row)
        cpu.verify_file(source, row)


def patch_roster(source, frontend, lock):
    placement = lock["final_placement"]
    public_api = {"path": CAPI, "before_sha256": placement["after_sha256"], "after_sha256": PUBLIC_API_SOURCE}
    collector = {"path": CAPI, "before_sha256": PUBLIC_API_SOURCE, "after_sha256": COLLECTOR_SOURCE}
    frontend_upstream = source / "cmake/patches/cudnn_frontend/cudnn_frontend_win_arm64_compat.patch"
    return (
        ("loader", source, BUILD / "ort-windows-loader-proposal-r2.patch",
         lock["loader_patch_sha256"], lock["loader_files"]),
        ("offline", source, BUILD / "r8/offline-helper.patch",
         lock["offline_helper"]["patch_sha256"], [lock["offline_helper"]]),
        ("final-placement", source, BUILD / "r8/final-placement.patch",
         placement["patch_sha256"], [placement]),
        ("initialized-public-api", source, BUILD / "r12/initialized-public-api.patch",
         PUBLIC_API_PATCH, [public_api]),
        ("final-graph", source, BUILD.parent / "final-graph-metadata/collector.patch", COLLECTOR_PATCH, [collector]),
        ("frontend-arm64", frontend, frontend_upstream,
         lock["frontend_upstream_arm64"]["patch_sha256"], [lock["frontend_upstream_arm64"]]),
        ("frontend-authenticated-imports", frontend, BUILD / "r8/frontend-authenticated-imports.patch",
         lock["frontend_final"]["patch_sha256"], [lock["frontend_final"]]),
    )


def patch_transport(stage, original):
    if stage != "final-graph":
        return original
    normalized = original.replace(b"\r\n", b"\n")
    if b"\r" in normalized or hashlib.sha256(normalized).hexdigest() != COLLECTOR_LF_PATCH:
        raise ValueError("collector patch transport differs from reviewed LF projection")
    return normalized


def apply_patch(cpu, r8, patch_tool, snapshots, spec, run_patch, guard_paths, deadline):
    stage, root, patch, expected, targets = spec
    check_deadline(deadline)
    patch = cpu.plain(patch, regular=True)
    with patch.open("rb") as stream:
        original = stream.read(MAX_PATCH_BYTES + 1)
    original_pin = pin_bytes(patch, original)
    if len(original) > MAX_PATCH_BYTES or original_pin["sha256"] != expected:
        raise ValueError("reviewed CUDA source patch differs: " + stage)
    body = patch_transport(stage, original)
    snapshot = snapshots / (stage + ".patch")
    guard_paths([snapshot], False, deadline)
    with snapshot.open("xb") as stream:
        stream.write(body)
    guard_paths([snapshot], True, deadline)
    snapshot_pin = pin_bytes(snapshot, body)
    cpu.verify_file(snapshot, snapshot_pin)
    paths = [cpu.child(root, row["path"]) for row in targets]
    collector_base = None

    def dispatch():
        nonlocal collector_base
        if stage == "final-graph":
            collector_base = paths[0].read_bytes()
        guard_paths(paths, False, deadline)
        run_patch(
            [str(patch_tool), "--batch", "--forward", "--fuzz=0", "--binary",
             "--ignore-whitespace", "--no-backup-if-mismatch", "--reject-file=-",
             "-p1", "--input=" + str(snapshot)],
            root, stage,
        )
        guard_paths(paths, True, deadline)
        check_deadline(deadline)

    r8.after_patch_target_checks(cpu, root, targets, dispatch)
    for target, row in zip(paths, targets):
        after = row.get("after_sha256") or row["proposed_sha256"]
        if cpu.digest(target) != after:
            raise ValueError("reviewed CUDA patch output differs: " + row["path"])
    if collector_base is not None and not paths[0].read_bytes().startswith(collector_base):
        raise ValueError("collector changed the authenticated placement source prefix")
    cpu.verify_file(patch, original_pin)
    cpu.verify_file(snapshot, snapshot_pin)
    return {"stage": stage, "original": original_pin, "snapshot": snapshot_pin}


def verify_transformed_inventory(before, after, changed):
    previous = {row["path"]: row for row in before}
    actual = {row["path"]: row for row in after}
    if previous.keys() != actual.keys() or not changed <= previous.keys():
        raise ValueError("CUDA patch changed the approved source roster")
    if any(previous[path] != actual[path] for path in previous.keys() - changed):
        raise ValueError("CUDA patch modified an undeclared source file")


def materialize(*, destination, source_archive, dependency_archives, lock, cpu, r8,
                patch_tool, run_patch, guard_paths, deadline):
    """Extract sealed inputs into an empty source namespace and apply exact overlays.

    The caller owns the fresh destination, authenticated patch-tool pin, and
    absolute operation deadline. Both callbacks must stay within that deadline.
    No historical entry point, compiler, existing build output or network is used.
    """
    check_deadline(deadline)
    destination = cpu.plain(destination)
    patch_tool = cpu.plain(patch_tool, regular=True)
    source, frontend, snapshots = (destination / name for name in ("source", "frontend", "patch-inputs"))
    mirror = source / "mirror"
    source_receipt = destination / "source-files.json"
    if not destination.is_dir() or any(path.exists() for path in (source, frontend, snapshots, source_receipt)):
        raise ValueError("materialization requires an existing guarded fresh source destination")
    if lock["target"] != TARGET or lock["ort_version"] != cpu.VERSION or lock["upstream_commit"] != cpu.COMMIT:
        raise ValueError("CUDA lock does not match its canonical ORT source owner")

    cpu_dependencies = cpu.selected_dependencies(TARGET)
    cuda_dependencies = lock["dependencies"]
    dependencies = cpu_dependencies + cuda_dependencies
    names = {row["name"] for row in dependencies}
    if len(names) != len(dependencies) or set(dependency_archives) != names:
        raise ValueError("exact offline dependency archive roster required")
    source_archive = cpu.plain(source_archive, regular=True)
    cpu.verify_file(source_archive, cpu.LOCK["source"])
    for row in dependencies:
        cpu.verify_file(dependency_archives[row["name"]], row)
    guard_paths([source, frontend, snapshots, source_receipt], False, deadline)
    checked_cpu = audited_cpu(cpu, guard_paths, deadline)
    checked_cpu.extract_source(source_archive, destination, cpu.LOCK["source"])
    cpu.verify_file(source_archive, cpu.LOCK["source"])

    ensure_directory(mirror, source, guard_paths, deadline)
    mirror_inputs(cpu, mirror, cpu_dependencies, dependency_archives, guard_paths, deadline)
    before_source = cpu.verify_source(destination, TARGET)
    mirror_inputs(cpu, mirror, cuda_dependencies, dependency_archives, guard_paths, deadline)
    expected_mirror = [
        {"path": cpu.mirror_relative(row), "bytes": row["bytes"], "sha256": row["sha256"]}
        for row in dependencies
    ]
    r8.verify_mirror_closure(cpu, mirror, expected_mirror)
    check_deadline(deadline)

    frontend_input = dependency_archives["cudnn_frontend"]
    before_frontend = r8.safe_frontend(checked_cpu, frontend_input, frontend, lock)["files_before_patches"]
    ensure_directory(snapshots, destination, guard_paths, deadline)
    patches = patch_roster(source, frontend, lock)
    patch_inputs = [
        apply_patch(cpu, r8, patch_tool, snapshots, item, run_patch, guard_paths, deadline)
        for item in patches
    ]
    source_inventory = checked_cpu.source_inventory(source, exclude_mirror=True)
    frontend_inventory = checked_cpu.source_inventory(frontend)
    mirror_inventory = checked_cpu.source_inventory(mirror)
    for root, before, after in (
        (source, before_source, source_inventory), (frontend, before_frontend, frontend_inventory),
    ):
        changed = {row["path"] for _, tree, _, _, rows in patches if tree == root for row in rows}
        verify_transformed_inventory(before, after, changed)
    if mirror_inventory != sorted(expected_mirror, key=lambda row: row["path"]):
        raise ValueError("offline CUDA mirror changed during source transformation")
    collector = next(row for row in source_inventory if row["path"] == CAPI)
    if collector["sha256"] != COLLECTOR_SOURCE:
        raise ValueError("final CUDA collector source differs")
    check_deadline(deadline)
    return {
        "source": source, "frontend": frontend, "mirror": mirror,
        "source_inventory": source_inventory, "frontend_inventory": frontend_inventory,
        "mirror_inventory": mirror_inventory, "patch_inputs": patch_inputs,
        "collector_source": dict(collector, path=str(source / CAPI)),
    }
