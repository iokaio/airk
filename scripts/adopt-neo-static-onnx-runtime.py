# SPDX-License-Identifier: Apache-2.0
"""Adopt one reviewed, completed Neo build without running a compiler or build."""
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import time

ROOT = "/Users/tylerjensen/airk-builds/one-airk-static-ort-1814-1794087059"
TARGET = "aarch64-apple-darwin"
MODE = "adopted_reviewed_neo_arm64"
PRODUCER = "evidence2/receipt.json"
EVIDENCE = {
    PRODUCER: "40163d30db4fd0104dd0545c6ddb3b0eaad7b52260251c13c3693f1fc283d35f",
    "evidence/receipt.json": "8cf93f568a5e643542a9f609a219c320247317dcee407a57329a974530bcff59",
    "postbuild-evidence/archive-inspection.json": "cc430c5dcbb700b067588f2c325bb4e4df210610b919e849a4f0106d4e92c7bd",
}
QUALIFICATION = {"fresh_build": False, "final_static_link": False, "actual_model": False, "cross_platform": False}


def fixed_root(destination, target, core):
    if platform.system() != "Darwin" or platform.machine() != "arm64" or target != TARGET:
        raise ValueError("Reviewed Neo adoption requires native macOS ARM64")
    destination = core.plain(destination)
    if str(destination) != ROOT:
        raise ValueError("Reviewed Neo build cannot be moved or path-rewritten")
    return destination


def source_inventory(source, core):
    """Hash all original LF bytes, pruning only the documented Git/mirror roots."""
    source = core.plain(source)
    rows = []
    for directory, directories, files in os.walk(source, followlinks=False):
        if Path(directory) == source:
            directories[:] = [name for name in directories if name not in {".git", "mirror"}]
        for name in directories:
            core.plain(Path(directory) / name)
        for name in files:
            path = core.plain(Path(directory) / name, regular=True)
            size = path.stat().st_size
            if size > 128 * 1024 * 1024 or len(rows) >= 12000:
                raise ValueError("Neo source inventory exceeds bounds")
            rows.append({"path": path.relative_to(source).as_posix(), "bytes": size, "sha256": core.digest(path)})
    return sorted(rows, key=lambda row: row["path"])


def verify_source(destination, core):
    rows = source_inventory(destination / "onnxruntime-source", core)
    seal = hashlib.sha256((json.dumps(rows, indent=2, sort_keys=True) + "\n").encode()).hexdigest()
    if len(rows) != core.LOCK["source"]["files"] or seal != core.LOCK["source"]["inventory_sha256"]:
        raise ValueError("Neo compiled source is not the complete sealed upstream LF tree")
    return seal


def validate_inspection(inspection, original):
    if (inspection.get("producer_receipt_sha256") != EVIDENCE[PRODUCER]
            or inspection.get("result") != "archive_metadata_verified"
            or inspection.get("library_count") != 79
            or any(inspection.get(key) is not True for key in ("all_archives_arm64", "libraries_hash_match", "all_original_library_hashes_still_match"))):
        raise ValueError("Neo archive inspection is incomplete")
    expected = {ROOT + "/b2/Release/" + row["path"] for row in original["libraries"]}
    observed = []
    for command in inspection["commands"]:
        if command["exit_code"] != 0:
            raise ValueError("Neo archive inspection command failed")
        if command["argv"][:2] == ["/usr/bin/lipo", "-archs"]:
            if len(command["argv"]) != 3 or command["stdout"].strip() != "arm64":
                raise ValueError("Neo archive architecture differs")
            observed.append(command["argv"][2])
    if len(observed) != 79 or set(observed) != expected:
        raise ValueError("Neo archive architecture coverage differs")
    object_row = inspection["representative_object"]
    if (object_row["minimum_os"] != "13.3" or object_row["archive"] != "libonnxruntime_session.a"
            or object_row["member"] != "IOBinding.cc.o"):
        raise ValueError("Neo representative object identity differs")
    otool = [row for row in inspection["commands"] if row["argv"][:2] == ["/usr/bin/otool", "-l"]]
    if len(otool) != 1 or not re.search(r"minos\s+13\.3\s+sdk\s+26\.5", otool[0]["stdout"]):
        raise ValueError("Neo object deployment target/actual SDK differs")


def validate_compiler_metadata(metadata):
    if 'set(CMAKE_CXX_COMPILER_VERSION "21.0.0.21000101")' not in metadata:
        raise ValueError("Neo compiled C++ metadata differs")


def inputs(destination, target, producer_receipt, core):
    core.validate_lock()
    destination = fixed_root(destination, target, core)
    if core.plain(producer_receipt, regular=True) != destination / PRODUCER:
        raise ValueError("Neo producer receipt must retain its original location")
    for name, seal in EVIDENCE.items():
        if core.digest(core.plain(core.child(destination, name), regular=True)) != seal:
            raise ValueError("Changed or unrecognized Neo original evidence: " + name)
    original = core.read_json(destination / PRODUCER)
    failed = core.read_json(destination / "evidence/receipt.json")
    inspection = core.read_json(destination / "postbuild-evidence/archive-inspection.json")
    if (original["upstream_commit"] != core.COMMIT or original["ort_version"] != core.VERSION
            or original["architecture"] != "arm64" or original["deployment_target"] != "13.3"
            or original["result"] != "static_libraries_built_not_AIRK_model_or_link_qualified"
            or len(original["libraries"]) != 79 or any(row["exit_code"] != 0 for row in original["commands"])):
        raise ValueError("Neo original build identity/result differs")
    validate_inspection(inspection, original)
    source_seal = verify_source(destination, core)
    source = destination / "onnxruntime-source"
    for relative, seal in (("inputs/upstream-metadata.json", original["source_metadata_sha256"]),
                           ("inputs/cpu-dependencies.json", original["dependency_manifest_sha256"])):
        if core.digest(core.plain(destination / relative, regular=True)) != seal:
            raise ValueError("Neo source/dependency provenance changed")
    dependencies = core.selected_dependencies(target)
    mirror = source / "mirror"
    for row in dependencies:
        core.verify_file(core.child(mirror, core.mirror_relative(row)), row)
    if {row["path"] for row in core.source_inventory(mirror)} != {core.mirror_relative(row) for row in dependencies}:
        raise ValueError("Neo dependency mirror roster differs")
    libraries = core.library_inventory(destination, target, build_relative="b2/Release")
    expected = [{**row, "path": "b2/Release/" + row["path"]} for row in original["libraries"]]
    if sorted(libraries, key=lambda row: row["path"]) != sorted(expected, key=lambda row: row["path"]):
        raise ValueError("Neo original static archive bytes/roster changed")
    cache = core.verify_cmake_cache(destination / "b2/Release/CMakeCache.txt", original["configure"])
    for key in ("CMAKE_HOME_DIRECTORY", "onnxruntime_SOURCE_DIR"):
        if key in cache and cache[key] != str(source / "cmake"):
            raise ValueError("Neo CMake source root differs")
    compiler = Path(cache["CMAKE_CXX_COMPILER"])
    if str(compiler) not in {"/usr/bin/clang++", "/Library/Developer/CommandLineTools/usr/bin/clang++"}:
        raise ValueError("Neo compiler path differs")
    # Original producer output identifies the compiled toolchain. Current tool
    # hashes below are adoption observations, not claims of a fresh compile.
    if not original["toolchain"]["clang"].startswith("Apple clang version 21.0.0 (clang-2100.1.1.101)\nTarget: arm64-"):
        raise ValueError("Neo original compiler identity differs")
    metadata = core.plain(destination / "b2/Release/CMakeFiles/3.31.6/CMakeCXXCompiler.cmake", regular=True).read_text()
    validate_compiler_metadata(metadata)
    ninja = core.plain(destination / "b2/Release/build.ninja", regular=True).read_text()
    if not re.search(r"-arch\s+arm64", ninja) or "-mmacosx-version-min=13.3" not in ninja:
        raise ValueError("Neo actual compile flags differ")
    # The SDK path is an Apple-managed alias; retain the actual resolved identity.
    sdk = Path(cache["CMAKE_OSX_SYSROOT"]).resolve(strict=True)
    settings = json.loads((sdk / "SDKSettings.json").read_bytes())
    if settings.get("Version") != "26.5":
        raise ValueError("Neo actual SDK version changed (deployment minimum is separately 13.3)")
    tools = {}
    for name in ("cmake", "ninja", "python"):
        path = Path(original["python"]["path"] if name == "python" else original["tools"][name])
        seal = original["python"]["sha256"] if name == "python" else original["tools"][name + "_sha256"]
        if core.digest(core.plain(path, regular=True)) != seal:
            raise ValueError("Neo original build tool changed: " + name)
        tools[name] = core.tool_record(path)
    tools["compiler"] = core.tool_record(compiler.resolve(strict=True))
    # Audit all notice inputs before creating any ownership/receipt records.
    core.notice_items(destination, target, source_root=source, dependency_root=mirror)
    return {"original_build": original, "original_failed_attempt": failed, "archive_inspection": inspection,
            "source_inventory_sha256": source_seal, "libraries": libraries, "adoption_tool_observations": tools,
            "actual_sdk": {"path": str(sdk), "version": "26.5"}, "deployment_target": "13.3"}


def environment(destination):
    return {"LIBONNXRUNTIME_NO_PKG_CONFIG": "1", "ORT_SKIP_DOWNLOAD": "1", "ORT_PREFER_DYNAMIC_LINK": "0",
            "ORT_LIB_LOCATION": str(destination / "b2"), "ORT_LIB_PROFILE": "Release"}


def validate_receipt(receipt, target, observed, core):
    if (receipt.get("schema") != 1 or receipt.get("build_mode") != MODE or receipt.get("target") != target
            or receipt.get("ort_version") != core.VERSION or receipt.get("upstream_commit") != core.COMMIT
            or receipt.get("result") != observed["original_build"]["result"]
            or receipt.get("lock_sha256") != core.digest(core.LOCK_PATH)
            or receipt.get("original_evidence_sha256") != EVIDENCE or receipt.get("qualification") != QUALIFICATION):
        raise ValueError("Adopted Neo ownership/input/qualification identity differs")
    if any(receipt.get(key) != value for key, value in observed.items()):
        raise ValueError("Adopted Neo original inputs or build evidence changed")


def adopt(destination, target, producer_receipt, core):
    destination = fixed_root(destination, target, core)
    if core.plain(producer_receipt) != destination / PRODUCER:
        raise ValueError("Neo producer receipt must retain its original location")
    if (destination / "owner.json").exists():
        if core.read_json(destination / "owner.json") != core.owner_value(target):
            raise ValueError("Neo destination owner differs")
        return verify(destination, target, core.read_json(destination / "build-receipt.json"), core)
    observed = inputs(destination, target, producer_receipt, core)
    receipt_path = destination / "build-receipt.json"
    if receipt_path.exists():
        # A completed receipt whose owner write was interrupted can resume only
        # after the entire original input audit and exact notice audit pass.
        receipt = core.read_json(receipt_path)
        validate_receipt(receipt, target, observed, core)
        notices = core.collect_notices(destination, target, verify_only=True, source_root=destination / "onnxruntime-source",
                                       dependency_root=destination / "onnxruntime-source/mirror")
        if receipt.get("notices") != notices:
            raise ValueError("Interrupted Neo adoption notices differ")
        core.write_json(destination / "owner.json", core.owner_value(target))
        return {"receipt": receipt, "environment": environment(destination)}
    receipt = {"schema": 1, "build_mode": MODE, "target": target, "ort_version": core.VERSION,
               "result": observed["original_build"]["result"],
               "upstream_commit": core.COMMIT, "lock_sha256": core.digest(core.LOCK_PATH),
               "original_evidence_sha256": dict(EVIDENCE), "qualification": dict(QUALIFICATION),
               "adopted_unix_seconds": time.time(), **observed,
               "notices": core.collect_notices(destination, target, source_root=destination / "onnxruntime-source",
                                              dependency_root=destination / "onnxruntime-source/mirror")}
    # The original failed and corrected builds and their timing records stay intact.
    core.write_json(destination / "build-receipt.json", receipt)
    core.write_json(destination / "owner.json", core.owner_value(target))
    return {"receipt": receipt, "environment": environment(destination)}


def verify(destination, target, receipt, core):
    destination = fixed_root(destination, target, core)
    if core.read_json(destination / "owner.json") != core.owner_value(target):
        raise ValueError("Adopted Neo ownership differs")
    observed = inputs(destination, target, destination / PRODUCER, core)
    validate_receipt(receipt, target, observed, core)
    notices = core.collect_notices(destination, target, verify_only=True, source_root=destination / "onnxruntime-source",
                                   dependency_root=destination / "onnxruntime-source/mirror")
    if receipt.get("notices") != notices:
        raise ValueError("Adopted Neo full license notices changed")
    return {"receipt": receipt, "environment": environment(destination)}
