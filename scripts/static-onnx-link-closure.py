# SPDX-License-Identifier: Apache-2.0
"""Pinned ort-sys rc10 static search-layout qualification and additive re2 build.

This checks archives requested by the pinned consumer, not successful symbol
resolution or model inference. Actual final linking remains a separate gate.
"""
import copy
import os
from pathlib import Path
import subprocess
import time

SUPPLEMENT = "link-supplement-receipt.json"
RECOVERY = "link-supplement-recovery.json"
NEO_RECOVERY = {
    "link-supplement-attempt.json": "25ba4cd7611d1636961cb88080b33bebaf528d06032deb1a5b87a3bac8b70ef5",
    "logs/link-supplement.log": "c6db0cbdf009cb15aa4c5ccf31e0397cc73564f19b10b1dd6f65bc4f14b1d703",
    "build-receipt.json": "1973d3a7252c60f17a8ac898e709cb5b61775134ea34a8376064b627108b023d",
}
RECOVERED_LIBRARY = {"path": "b2/Release/_deps/re2-build/libre2.a", "bytes": 662656,
                     "sha256": "45b7e2437cca55703e4aa320c3e9d691d2e3607c6ea7ac6557a3f99faee74cc0"}
CONSUMER_BUILD_RS_SHA256 = "c1e166d9f3025251e755ea98bbcce4e2e15d6cc19fd57afc5ef2951e2aca6995"
ABSL = {
    "debugging": "examine_stack debugging_internal demangle_internal demangle_rust decode_rust_punycode utf8_for_code_point symbolize stacktrace",
    "base": "base spinlock_wait malloc_internal strerror raw_logging_internal throw_delegate",
    "hash": "hash city low_level_hash",
    "container": "raw_hash_set",
    "synchronization": "kernel_timeout_internal graphcycles_internal synchronization",
    "time": "time_zone time",
    "numeric": "int128",
    "strings": "str_format_internal strings string_view strings_internal",
    "log": "log_globals log_internal_format log_internal_proto log_internal_globals log_internal_log_sink_set log_sink log_internal_message",
}


def layout(target, result):
    base = Path(result["environment"]["ORT_LIB_LOCATION"])
    return base / "Release", target.endswith("windows-msvc")


def requested_paths(target, result):
    """Exact non-vcpkg second static layout selected by rc10 for our b root."""
    build, windows = layout(target, result)
    archive = lambda name: name + ".lib" if windows else "lib" + name + ".a"
    paths = [("onnxruntime_" + component, build / archive("onnxruntime_" + component))
             for component in ("common", "flatbuffers", "framework", "graph", "lora", "mlas", "optimizer", "providers", "session", "util")]
    deps = build / "_deps"
    protobuf = deps / "protobuf-build"
    for name in ("protobuf-lited", "protobuf-lite", "protobuf"):
        candidate = protobuf / archive(("lib" if windows else "") + name)
        if candidate.exists():
            paths.append((candidate.stem, candidate))
    if not any(path.parent == protobuf for _name, path in paths):
        raise ValueError("Pinned ort-sys layout has no selected protobuf archive")
    paths.extend((name, deps / folder / archive(name)) for name, folder in
                 (("onnx", "onnx-build"), ("onnx_proto", "onnx-build"), ("cpuinfo", "pytorch_cpuinfo-build"), ("re2", "re2-build")))
    for folder, names in ABSL.items():
        paths.extend(("absl_" + name, deps / "abseil_cpp-build/absl" / folder / archive("absl_" + name)) for name in names.split())
    for name, parent in (("nsync_cpp", deps / "google_nsync-build"), ("clog", deps / "pytorch_cpuinfo-build/deps/clog"),
                         ("clog", deps / "pytorch_clog-build"), ("absl_log_internal_check_op", deps / "abseil_cpp-build/absl/log")):
        candidate = parent / archive(name)
        if candidate.exists():
            paths.append((name, candidate))
    return paths


def closure(destination, target, result, core):
    rows = []
    for name, path in requested_paths(target, result):
        core.plain(path, regular=True)
        size = path.stat().st_size
        if not 64 < size <= 1024 * 1024 * 1024:
            raise ValueError("Pinned ort-sys static archive size differs: " + name)
        with path.open("rb") as source:
            if source.read(8) != b"!<arch>\n":
                raise ValueError("Pinned ort-sys static archive header differs: " + name)
        rows.append({"name": name, "path": path.relative_to(destination).as_posix(), "bytes": size, "sha256": core.digest(path)})
    return {"schema": 1, "consumer": "ort-sys-2.0.0-rc.10", "consumer_build_rs_sha256": CONSUMER_BUILD_RS_SHA256,
            "layout": "profile-root-deps-untransformed", "archives": rows,
            "final_symbol_link_qualified": False, "model_qualified": False}


def original_inventory(destination, target, build_relative, rows, core):
    """The original immutable receipt continues to describe ONLY its old bytes."""
    path = destination / SUPPLEMENT
    if path.exists():
        receipt = core.read_json(path)
    elif (destination / RECOVERY).exists():
        receipt = core.read_json(destination / RECOVERY)
    elif (destination / "link-supplement-attempt.json").exists() and target == "aarch64-apple-darwin":
        # This separates the unchanged original producer roster from one exact
        # failed-run artifact. It grants NO consumer qualification: consumer()
        # still requires the explicit reviewed recovery receipt below.
        failed = pinned_failure(destination, target, core)
        receipt = {"schema": 1, "target": target, "original_build_receipt_sha256": NEO_RECOVERY["build-receipt.json"],
                   "result": "archive_recovered_after_owned_runner_failure", "library": RECOVERED_LIBRARY,
                   "original_libraries": failed["original_libraries"]}
    else:
        return rows
    if (receipt.get("schema") != 1 or receipt.get("target") != target
            or receipt.get("original_build_receipt_sha256") != core.digest(destination / "build-receipt.json")
            or receipt.get("result") not in {"supplemental_re2_archive_built_not_final_link_qualified", "archive_recovered_after_owned_runner_failure"}):
        raise ValueError("Supplemental static receipt binding differs")
    extra = receipt.get("library")
    expected = build_relative + "/_deps/re2-build/" + ("re2.lib" if target.endswith("windows-msvc") else "libre2.a")
    if not isinstance(extra, dict) or extra.get("path") != expected or extra not in rows:
        raise ValueError("Supplemental real re2 archive changed/missing")
    remaining = [row for row in rows if row != extra]
    if remaining != receipt.get("original_libraries"):
        raise ValueError("Original archives changed during supplemental build")
    return remaining


def consumer(destination, target, result, core):
    qualified = closure(destination, target, result, core)
    value = copy.deepcopy(result)
    value["static_link_closure"] = qualified
    if (destination / SUPPLEMENT).exists():
        receipt = core.read_json(destination / SUPPLEMENT)
        build, _windows = layout(target, result)
        expected_argv = [receipt["cmake"]["path"], "--build", str(build), "--config", "Release", "--target", "re2", "--parallel", "2"]
        if (receipt.get("argv") != expected_argv or receipt.get("closure") != qualified
                or not core.successful(receipt.get("command", {}))):
            raise ValueError("Supplemental static command/result/closure differs")
        core.verify_file(receipt["cmake"]["path"], receipt["cmake"], system_tool=True)
        if target.endswith("windows-msvc"):
            core.verify_file(receipt["vcvars"]["path"], receipt["vcvars"], system_tool=True)
        value["static_link_supplement"] = {"path": SUPPLEMENT, "sha256": core.digest(destination / SUPPLEMENT), "receipt": receipt}
    elif (destination / RECOVERY).exists():
        receipt = core.read_json(destination / RECOVERY)
        validate_recovery(destination, target, result, receipt, core)
        value["static_link_recovery"] = {"path": RECOVERY, "sha256": core.digest(destination / RECOVERY), "receipt": receipt}
    elif not any(Path(row["path"]).name in {"re2.lib", "libre2.a"} for row in result["receipt"]["libraries"]):
        raise ValueError("Real additional re2 needs its explicit supplemental/recovery provenance")
    return value


def pinned_failure(destination, target, core):
    if target != "aarch64-apple-darwin":
        raise ValueError("No reviewed supplemental recovery exists for this target")
    for relative, seal in NEO_RECOVERY.items():
        if core.digest(core.plain(destination / relative, regular=True)) != seal:
            raise ValueError("Reviewed failed supplemental evidence changed: " + relative)
    failed = core.read_json(destination / "link-supplement-attempt.json")
    if (failed.get("result") != "failed" or failed.get("error") != "[Errno 1] Operation not permitted"
            or "command" in failed or len(failed.get("original_libraries", [])) != 79):
        raise ValueError("Reviewed supplemental failure classification differs")
    core.verify_file(destination / RECOVERED_LIBRARY["path"], RECOVERED_LIBRARY)
    return failed


def owned_root_absence(destination):
    result = subprocess.run(["/bin/ps", "-axo", "pid,pgid,uid,state,command"], stdin=subprocess.DEVNULL,
                            capture_output=True, timeout=2, env={**os.environ, "LC_ALL": "C"})
    if result.returncode != 0 or len(result.stdout) > 4 * 1024 * 1024 or len(result.stderr) > 4096:
        raise ValueError("Owned build-root process observation unavailable/truncated")
    matches = []
    for line in result.stdout.decode("utf-8", errors="strict").splitlines()[1:]:
        fields = line.split(None, 4)
        if len(fields) != 5 or not fields[0].isdigit():
            raise ValueError("Invalid native process observation")
        if str(destination) in fields[4] and int(fields[0]) != os.getpid():
            matches.append({"pid": int(fields[0]), "pgid": fields[1], "uid": fields[2], "state": fields[3], "command": fields[4]})
    if matches:
        raise ValueError("Owned build-root process still observed; no archive recovery")
    return {"kind": "native_ps_owned_root_commandline_absence", "coverage": "command_line_root_match_only", "descendants_settlement_qualified": False, "matched_processes": [], "observation_only": True}


def validate_recovery(destination, target, result, receipt, core):
    failed = pinned_failure(destination, target, core)
    if (receipt.get("schema") != 1 or receipt.get("target") != target
            or receipt.get("result") != "archive_recovered_after_owned_runner_failure"
            or receipt.get("command_exit_unknown") is not True or "command" in receipt
            or receipt.get("evidence") != NEO_RECOVERY or receipt.get("library") != RECOVERED_LIBRARY
            or receipt.get("original_libraries") != failed["original_libraries"]
            or receipt.get("argv") != failed["argv"] or receipt.get("cmake") != failed["cmake"]
            or receipt.get("closure") != closure(destination, target, result, core)
            or receipt.get("qualification") != {"fresh_full_build": False, "command_pass": False, "final_static_link": False, "actual_model": False}
            or receipt.get("process_absence") != {"kind": "native_ps_owned_root_commandline_absence", "coverage": "command_line_root_match_only", "descendants_settlement_qualified": False, "matched_processes": [], "observation_only": True}):
        raise ValueError("Recovered supplemental artifact provenance differs/overstates proof")
    core.verify_file(receipt["cmake"]["path"], receipt["cmake"], system_tool=True)


def recover(destination, target, core):
    """Adopt ONLY the reviewed real Neo artifact; never rerun any build command."""
    destination = core.owned_destination(destination, target)
    if (destination / SUPPLEMENT).exists():
        raise ValueError("Normal supplemental receipt already exists")
    if (destination / RECOVERY).exists():
        return core.verify_build(destination, target)
    failed = pinned_failure(destination, target, core)
    original = core.verify_original_build(destination, target)
    rows = core.library_inventory(destination, target, "b2/Release", include_supplement=True)
    if [row for row in rows if row != RECOVERED_LIBRARY] != failed["original_libraries"]:
        raise ValueError("Recovered archive is not the sole addition to original79")
    receipt = {"schema": 1, "target": target, "result": "archive_recovered_after_owned_runner_failure",
               "original_build_receipt_sha256": NEO_RECOVERY["build-receipt.json"], "evidence": NEO_RECOVERY,
               "library": RECOVERED_LIBRARY, "original_libraries": failed["original_libraries"], "argv": failed["argv"], "cmake": failed["cmake"],
               "command_exit_unknown": True, "process_absence": owned_root_absence(destination),
               "closure": closure(destination, target, original, core),
               "qualification": {"fresh_full_build": False, "command_pass": False, "final_static_link": False, "actual_model": False}}
    validate_recovery(destination, target, original, receipt, core)
    core.write_json(destination / RECOVERY, receipt)
    return core.verify_build(destination, target)


def supplement(destination, target, core):
    destination = core.owned_destination(destination, target)
    if (destination / SUPPLEMENT).exists():
        return core.verify_build(destination, target)
    result = core.verify_original_build(destination, target)
    original = result["receipt"]
    if original.get("build_mode") not in {"adopted_reviewed_windows_c1", "adopted_reviewed_neo_arm64"}:
        raise ValueError("Supplement applies only to the two exact reviewed old campaigns")
    build, windows = layout(target, result)
    relative = build.relative_to(destination).as_posix()
    original_rows = core.library_inventory(destination, target, relative, include_supplement=True)
    if any(Path(row["path"]).name in {"re2.lib", "libre2.a"} for row in original_rows):
        raise ValueError("Existing real re2 needs no supplemental build")
    tools = original["adoption_tool_observations"]
    cmake = tools["cmake"]
    core.verify_file(cmake["path"], cmake, system_tool=True)
    argv = [cmake["path"], "--build", str(build), "--config", "Release", "--target", "re2", "--parallel", "2"]
    attempt = destination / "link-supplement-attempt.json"
    receipt = {"schema": 1, "target": target, "result": "running", "original_build_receipt_sha256": core.digest(destination / "build-receipt.json"),
               "original_libraries": original_rows, "argv": argv, "cmake": cmake, "qualification": {"fresh_full_build": False, "final_static_link": False, "actual_model": False}}
    core.write_json(attempt, receipt)
    environment = dict(os.environ)
    for key in list(environment):
        if key in {"CC", "CXX", "CFLAGS", "CXXFLAGS", "CPPFLAGS", "LDFLAGS", "PYTHONPATH", "PYTHONHOME"} or key.startswith("CMAKE_"):
            environment.pop(key)
    environment["PYTHONNOUSERSITE"] = "1"
    environment["PATH"] = str(Path(tools["python"]["path"]).parent) + os.pathsep + environment.get("PATH", "")
    started = time.monotonic()
    try:
        if windows:
            cache = core.verify_cmake_cache(build / "CMakeCache.txt", [])
            vcvars = Path(cache["CMAKE_CXX_COMPILER"]).parents[6] / "Auxiliary/Build/vcvars64.bat"
            receipt["vcvars"] = core.tool_record(vcvars)
            actual = core.windows_command(argv, {"vcvars": receipt["vcvars"], "windows_sdk": "10.0.26100.0", "msvc_toolset": "14.44.35207"}, destination, "link-supplement")
        else:
            environment["MACOSX_DEPLOYMENT_TARGET"] = "13.3"
            actual = argv
        receipt["command"] = core.run_owned(actual, destination, destination / "logs/link-supplement.log", 1200, environment)
        if not core.successful(receipt["command"]):
            raise ValueError("Supplemental re2 build failed/descendants unsettled")
        rows = core.library_inventory(destination, target, relative, include_supplement=True)
        for row in original_rows:
            if row not in rows:
                raise ValueError("Supplemental build changed an original archive")
        extras = [row for row in rows if row not in original_rows]
        name = relative + "/_deps/re2-build/" + ("re2.lib" if windows else "libre2.a")
        if len(extras) != 1 or extras[0]["path"] != name:
            raise ValueError("Supplemental build produced unexpected archive changes")
        receipt["library"] = extras[0]
        receipt["closure"] = closure(destination, target, result, core)
        receipt["result"] = "supplemental_re2_archive_built_not_final_link_qualified"
        receipt["elapsed_seconds"] = round(time.monotonic() - started, 3)
        core.write_json(attempt, receipt, replace=True)
        core.write_json(destination / SUPPLEMENT, receipt)
        return core.verify_build(destination, target)
    except Exception as error:
        receipt["result"] = "failed"
        receipt["error"] = str(error)
        receipt["elapsed_seconds"] = round(time.monotonic() - started, 3)
        core.write_json(attempt, receipt, replace=True)
        raise
