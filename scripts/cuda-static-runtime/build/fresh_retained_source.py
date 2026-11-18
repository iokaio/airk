# SPDX-License-Identifier: Apache-2.0
"""Reconstruct fresh CUDA source facts from retained, read-only evidence."""

import hashlib
import io
import math
from pathlib import Path
import stat
import zipfile


def require(condition, message):
    if not condition:
        raise ValueError(message)


def consume(stream, size, check, *, collect=False, upstream=False):
    hashes = [hashlib.sha256(), *([hashlib.sha1()] if upstream else [])]
    raw = bytearray() if collect else None
    remaining = size
    while remaining:
        check()
        block = stream.read(min(remaining, 1 << 20))
        require(block, "truncated retained source input")
        for digest in hashes:
            digest.update(block)
        if collect:
            raw.extend(block)
        remaining -= len(block)
    require(not stream.read(1), "retained source input exceeds its pinned length")
    check()
    return [digest.hexdigest() for digest in hashes], bytes(raw) if collect else None


def read_file(cpu, path, row, check, *, collect=False):
    path = cpu.plain(path, regular=True)
    size = row["bytes"]
    require(type(size) is int and size >= 0 and path.stat().st_size == size,
            "retained source input length differs: " + str(path))
    upstream = row.get("upstream_sha1")
    check()
    with path.open("rb") as stream:
        actual, raw = consume(stream, size, check, collect=collect, upstream=bool(upstream))
    require(actual == [row["sha256"], *([upstream] if upstream else [])],
            "retained source input hash differs: " + str(path))
    return raw


def read_bounded(cpu, path, limit, check):
    path = cpu.plain(path, regular=True)
    size = path.stat().st_size
    require(0 <= size <= limit, "retained source metadata exceeds its bound: " + str(path))
    check()
    with path.open("rb") as stream:
        digests, raw = consume(stream, size, check, collect=True)
    return raw, {"path": str(path), "bytes": size, "sha256": digests[0]}


def frontend_inventory(cpu, archive, lock, check):
    """Replay the pinned safe_frontend ZIP inventory without extracting members."""
    row = next(item for item in lock["dependencies"] if item["name"] == "cudnn_frontend")
    raw = read_file(cpu, archive, row, check, collect=True)
    with zipfile.ZipFile(io.BytesIO(raw)) as zipped:
        members = zipped.infolist()
        require(len(members) == row["entries"] and sum(info.file_size for info in members) == row["expanded_bytes"],
                "retained frontend ZIP expansion census differs")
        seen = set()
        skipped = []
        result = []
        for info in members:
            check()
            parts = cpu.relative_name(info.filename.rstrip("/")).parts
            require(parts[0] == "cudnn-frontend-1.27.0", "foreign retained frontend ZIP root")
            relative = "/".join(parts[1:])
            key = relative.casefold()
            require(key not in seen and not info.flag_bits & 1, "duplicate or encrypted retained frontend ZIP member")
            seen.add(key)
            mode = stat.S_IFMT(info.external_attr >> 16)
            if mode == stat.S_IFLNK:
                excluded = lock["excluded_link"]
                require(relative == excluded["path"] and info.file_size == excluded["bytes"],
                        "unreviewed frontend ZIP link")
                with zipped.open(info) as stream:
                    digests, _ = consume(stream, info.file_size, check)
                require(digests[0] == excluded["target_sha256"], "unreviewed frontend ZIP link target")
                skipped.append(relative)
                continue
            require(mode in (0, stat.S_IFREG, stat.S_IFDIR), "special retained frontend ZIP member")
            if not relative or info.is_dir():
                continue
            with zipped.open(info) as stream:
                digests, _ = consume(stream, info.file_size, check)
            result.append({"path": relative, "bytes": info.file_size, "sha256": digests[0]})
    require(skipped == [".claude/skills"], "retained frontend ZIP missing the exact excluded link")
    return sorted(result, key=lambda item: item["path"])


def observed_tree(cpu, pins, root, guard_paths, deadline, check):
    root = cpu.plain(root)
    require(root.is_dir(), "retained source directory missing: " + str(root))
    paths = [root]
    for path in root.rglob("*"):
        check()
        paths.append(path)
    guard_paths(paths, True, deadline)
    check()
    result = []
    for path in paths[1:]:
        check()
        path = cpu.plain(path)
        if path.is_file():
            actual = pins.pin(cpu.plain(path, regular=True), deadline)
            result.append({**actual, "path": path.relative_to(root).as_posix()})
        else:
            require(path.is_dir(), "special retained source tree entry: " + str(path))
    check()
    return sorted(result, key=lambda item: item["path"])


def patch_facts(cpu, sources, snapshots, specs, before, after, guard_paths, deadline, check):
    snapshot_paths = [snapshots / (stage + ".patch") for stage, *_ in specs]
    require(snapshots.is_dir() and set(snapshots.iterdir()) == set(snapshot_paths),
            "retained patch snapshot roster differs")
    guard_paths([snapshots, *snapshot_paths], True, deadline)
    current = {tree: {row["path"]: row["sha256"] for row in rows} for tree, rows in before.items()}
    changed = {tree: set() for tree in before}
    patch_inputs = []
    collector_patch = None
    for (stage, tree, original, expected, targets), snapshot in zip(specs, snapshot_paths):
        require(targets, "retained patch has no declared targets: " + stage)
        original_bytes, original_pin = read_bounded(cpu, original, sources.MAX_PATCH_BYTES, check)
        require(original_pin["sha256"] == expected, "retained original patch differs: " + stage)
        snapshot_bytes, snapshot_pin = read_bounded(cpu, snapshot, sources.MAX_PATCH_BYTES, check)
        require(snapshot_bytes == sources.patch_transport(stage, original_bytes),
                "retained patch transport differs: " + stage)
        patch_inputs.append({"stage": stage, "original": original_pin, "snapshot": snapshot_pin})
        for target in targets:
            relative = target["path"]
            cpu.relative_name(relative)
            require(current[tree].get(relative) == target["before_sha256"],
                    "retained patch before hash chain differs: " + relative)
            current[tree][relative] = target.get("after_sha256") or target["proposed_sha256"]
            changed[tree].add(relative)
        if stage == "final-graph":
            collector_patch = snapshot_bytes
    for tree in before:
        sources.verify_transformed_inventory(before[tree], after[tree], changed[tree])
        actual = {row["path"]: row["sha256"] for row in after[tree]}
        require(all(actual.get(relative) == current[tree][relative] for relative in changed[tree]),
                "retained terminal patch target hash differs: " + str(tree))
    return patch_inputs, collector_patch


def verify_collector_prefix(cpu, file, row, patch, sources, check):
    require(isinstance(patch, bytes), "retained final collector patch missing")
    lines = patch.splitlines(keepends=True)
    require(len(lines) > 3 and lines[0].startswith(b"--- ") and lines[1].startswith(b"+++ ")
            and lines[2].startswith(b"@@ "), "retained collector append patch header differs")
    added = []
    for line in lines[3:]:
        if line.startswith(b"+"):
            added.append(line[1:])
        else:
            require(not added and line.startswith(b" "), "retained collector patch is not a pure append")
    tail = b"".join(added)
    raw = read_file(cpu, file, row, check, collect=True)
    require(tail and raw.endswith(tail) and hashlib.sha256(raw[:-len(tail)]).hexdigest() == sources.PUBLIC_API_SOURCE,
            "retained collector changed the authenticated placement prefix")


def reconstruct(*, destination, source_archive, dependency_archives, lock, cpu, r8, sources, pins,
                guard_paths, deadline):
    """Reopen the exact materialized files under the original configure deadline."""
    require(type(deadline) in (int, float) and math.isfinite(deadline), "original retained source deadline required")

    def check():
        pins.deadline_check(deadline)

    check()
    destination = cpu.plain(destination)
    source, frontend, snapshots = (destination / name for name in ("source", "frontend", "patch-inputs"))
    mirror = source / "mirror"
    require(lock["target"] == sources.TARGET and lock["ort_version"] == cpu.VERSION
            and lock["upstream_commit"] == cpu.COMMIT, "retained CUDA source lock differs")
    cpu_dependencies = cpu.selected_dependencies(sources.TARGET)
    dependencies = cpu_dependencies + lock["dependencies"]
    names = {row["name"] for row in dependencies}
    require(len(names) == len(dependencies) and set(dependency_archives) == names,
            "exact retained offline dependency archive roster required")
    read_file(cpu, source_archive, cpu.LOCK["source"], check)
    before_frontend = frontend_inventory(cpu, dependency_archives["cudnn_frontend"], lock, check)
    for row in dependencies:
        if row["name"] != "cudnn_frontend":
            read_file(cpu, dependency_archives[row["name"]], row, check)

    base_path = destination / "source-files.json"
    guard_paths([base_path], True, deadline)
    base_raw, base_pin = read_bounded(cpu, base_path, cpu.MAX_JSON, check)
    require(base_pin["sha256"] == cpu.LOCK["source"]["inventory_sha256"],
            "retained ORT base inventory hash differs")
    before_source = pins.parse_json(base_raw)
    require(isinstance(before_source, list) and len(before_source) == cpu.LOCK["source"]["files"],
            "retained ORT base inventory count differs")
    inventory = {row["path"]: row for row in before_source}
    require(len(inventory) == len(before_source)
            and all(inventory.get(row["path"]) == row for row in cpu.LOCK["source_metadata"]),
            "retained ORT base source metadata differs")
    for row in before_source:
        require(set(row) == {"path", "bytes", "sha256"}, "invalid retained ORT base inventory row")
        cpu.relative_name(row["path"])

    entire_source = observed_tree(cpu, pins, source, guard_paths, deadline, check)
    source_rows = [row for row in entire_source if not row["path"].startswith("mirror/")]
    mirror_rows = [{**row, "path": row["path"][len("mirror/"):]} for row in entire_source
                   if row["path"].startswith("mirror/")]
    expected_mirror = sorted(
        [{"path": cpu.mirror_relative(row), "bytes": row["bytes"], "sha256": row["sha256"]}
         for row in dependencies], key=lambda item: item["path"],
    )
    require(mirror.is_dir() and mirror_rows == expected_mirror, "retained exact offline mirror closure differs")
    version = read_file(cpu, source / "VERSION_NUMBER", inventory["VERSION_NUMBER"], check, collect=True)
    require(version.decode("utf-8").strip() == cpu.VERSION, "retained ORT source version differs")
    deps_path = "cmake/deps.txt"
    deps_raw = read_file(cpu, source / deps_path, inventory[deps_path], check, collect=True)
    upstream = {}
    for line in deps_raw.decode("utf-8").splitlines():
        if line and not line.startswith("#"):
            name, url, sha1 = line.split(";")
            upstream[name] = (url, sha1)
    require(all(upstream.get(row["name"]) == (row["url"], row["upstream_sha1"]) for row in cpu_dependencies),
            "retained ORT dependency differs from the pinned deps.txt")
    frontend_rows = observed_tree(cpu, pins, frontend, guard_paths, deadline, check)
    patch_inputs, collector_patch = patch_facts(
        cpu, sources, snapshots, sources.patch_roster(source, frontend, lock),
        {source: before_source, frontend: before_frontend}, {source: source_rows, frontend: frontend_rows},
        guard_paths, deadline, check,
    )
    collector = next(row for row in source_rows if row["path"] == sources.CAPI)
    require(collector["sha256"] == sources.COLLECTOR_SOURCE, "retained final CUDA collector source differs")
    verify_collector_prefix(cpu, source / sources.CAPI, collector, collector_patch, sources, check)
    check()
    return {
        "source": source, "frontend": frontend, "mirror": mirror, "source_inventory": source_rows,
        "frontend_inventory": frontend_rows, "mirror_inventory": mirror_rows, "patch_inputs": patch_inputs,
        "collector_source": {**collector, "path": str(source / sources.CAPI)},
    }
