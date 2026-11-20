# SPDX-License-Identifier: Apache-2.0
"""Run one reviewed Compact FP16 diagnostic or scoring selector in a Windows Job."""

import argparse
import ctypes
from datetime import datetime, timedelta, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import re
import runpy
import struct
import sys
import time


ROOT = Path(__file__).resolve().parents[2]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
PREFIX = "inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::fp16_compact::"
SELECTORS = {
    "metadata": PREFIX + "emit_cuda_compact_fp16_probe_metadata",
    "graph": PREFIX + "cuda_compact_fp16_load_only_graph_diagnostic",
    "projection-metadata": PREFIX + "emit_cuda_compact_fp32_qkv_projection_probe_metadata",
    "projection-graph": PREFIX + "cuda_compact_fp32_qkv_projection_load_only_graph_diagnostic",
    "projection-t32": PREFIX + "cuda_compact_fp32_qkv_projection_t32_component",
    "qkv-resource20": PREFIX + "cuda_compact_fp16_resource20",
    "qkv-mixed": PREFIX + "cuda_compact_fp16_mixed_timing",
    "qkv-scientific-cal": PREFIX + "cal::compact_qkv_scientific_cal_original_readout",
    "qkv-observed-development": PREFIX + "cuda_compact_fp16_observed_development",
    "qkv-fit-probe": PREFIX + "cuda_compact_fp16_fit_probe",
    "qkv-development": PREFIX + "cuda_compact_fp32_qkv_projection_development_batch",
    "qkv-decision-t1024": PREFIX + "cuda_compact_fp32_qkv_decision_t1024",
    "qkv-layer0-t1024": PREFIX + "cuda_compact_fp32_qkv_layer0_t1024",
    "full-cuts": PREFIX + "full::observe_full_cuts",
    "full-observed": PREFIX + "full::observe_full_scores",
    "full-metadata": PREFIX + "full::emit_full_metadata",
    "full-graph": PREFIX + "full::observe_full_graph",
    "down-metadata": PREFIX + "down::emit_down_metadata",
    "down-graph": PREFIX + "down::observe_down_graph",
    "down-observed": PREFIX + "down::observe_down_scores",
    "qkvo-metadata": PREFIX + "emit_cuda_compact_fp32_qkvo_projection_probe_metadata",
    "qkvo-graph": PREFIX + "cuda_compact_fp32_qkvo_projection_load_only_graph_diagnostic",
    "qkvo-t32": PREFIX + "cuda_compact_fp32_qkvo_projection_t32_component",
    "t32": PREFIX + "cuda_compact_fp16_t32_component",
    "bounded-t32": PREFIX + "cuda_compact_fp16_bounded_t32_component",
    "layer0-t32": PREFIX + "cuda_compact_fp16_layer0_taps",
    "layer0-projection-t32": PREFIX + "cuda_compact_fp16_layer0_projection_taps",
}
FULL_SCORE = "full-observed"
FULL_CUT = "full-cuts"
FULL_OBSERVED = (FULL_SCORE, FULL_CUT)
FULL_MODES = ("full-metadata", "full-graph", *FULL_OBSERVED)
FULL_NATIVE_SHA = "492da42c6b55364d79daa2b20b1d17373039acc5c43995c4df11031fa82e4c54"
FULL_GRAPH_RECEIPT_SHA = "606ca745e9ba145826694fa2aa5633aab5bd567538e00f98fe9e81cd0079015f"
FULL_REFERENCE_SHA = "0995832e12ef5a84af11562187281c86b96304693053272b3b1fbd091026afc7"
FULL_STAGE_SHA = "6486768c16820bc09f11c2dfbbbdd1d0ddf896e3a10b2315ec9f5d798ae9add1"
FULL_GRAPH_SHA = "fe94664a4addf797feaa54235e07f484f4e72051a894a154e5aff7f2c1c11ea6"
FULL_STAGE_SCHEMA = "airk.private.compact-full-projection-candidate/1"
DOWN_STAGE_SCHEMA = "airk.private.compact-fp32-qkv-down-projection-candidate/1"
DOWN_STAGE_SHA = "95e81ed4222c4cfdfc57e7e5f4600f135fe9bee9cbc7f7439b6111543476a737"
DOWN_GRAPH_SHA = "a583cbd0906a9801461ac0678d1607ecfe73398b637c19c97ea072c5750a33a1"
DOWN_SCORE = "down-observed"
DOWN_NATIVE_SHA = "295f0c79716cba2ec3da993e4a2659a7a67b6daec8bb473185c5818413d6daa4"
DOWN_GRAPH_RECEIPT_SHA = "c6d53799992c313daf988334547bd9d36ee5d5a99af6eb9375b01b375c186693"
DOWN_REFERENCE_SHA = "ffda904c92a2354ed9a4c17cf27ea8ce05452b56172c9d36c3a1c045a13ecf52"
DOWN_MODES = ("down-metadata", "down-graph", DOWN_SCORE)
QKVO_MODES = ("qkvo-metadata", "qkvo-graph", "qkvo-t32")
DEVELOPMENT_MODE = "qkv-development"
WIDER_TAP_MODE = "qkv-layer0-t1024"
DECISION_MODE = "qkv-decision-t1024"
WIDE_CAPTURES = (WIDER_TAP_MODE, DECISION_MODE)
DECISION_ROSTER = tuple((f"/airk_g5/weight//scorer/layers.{layer}/{site}/MatMul/input16", 2048)
                        for layer in (0, 9, 18, 27) for site in ("self_attn/k_proj",))
TIMING_MODES = ("qkv-resource20", "qkv-mixed", "qkv-fit-probe")
CAL_MODE = "qkv-scientific-cal"
OBSERVED_MODE = "qkv-observed-development"
OBSERVED_REFERENCES = {"f657bbcdc9dac88829376c639bf2f20b6e4ae4c40f796f6305c1ec044bb59b91",
                       "89b5a289b66ce93ac314c583cadff2112fda185081152d3f6a7886bae8b8d169"}
WIDER_REFERENCE_MODES = (OBSERVED_MODE, DEVELOPMENT_MODE, *WIDE_CAPTURES, *TIMING_MODES)
PROJECTION_SCORE_MODES = (CAL_MODE, "projection-t32", "qkvo-t32", DOWN_SCORE, *FULL_OBSERVED, *WIDER_REFERENCE_MODES)
PROJECTION_MODES = (CAL_MODE, "projection-metadata", "projection-graph", "projection-t32", *WIDER_REFERENCE_MODES, *QKVO_MODES, *DOWN_MODES, *FULL_MODES)
WIDER_TAP_SCHEMA = "airk.private.compact-fp32-qkv-layer0-t1024/1"
WIDER_TAP_POLICY = b"airk.private.compact-fp32-qkv-cuda-layer0-t1024/1"
SEALED_T1024_BATCH = "compact-fp16-qkv-fp32-native-development-r1"
SEALED_T1024_EXECUTION_SHA = "ebf0f2e1eadd7ff543479a254ed81240af4830a1cc9c1c78b1db69e5f1841ade"
SEALED_T1024_NATIVE_SHA = "4da43ecdfaeba452cc65009bcadfb82de942253c89a7d544c6285ad90855993a"
WIDER_TAP_TOKENS = 1024
DEVELOPMENT_SCHEMA = "airk.private.compact-fp32-qkv-development-batch/1"
DEVELOPMENT_REFERENCE_SCHEMA = "airk.private.compact-fp32-qkv-wider-reference/1"
DEVELOPMENT_REFERENCE_SHA = "f0d0e01fa28e7a876f1c57bc9894dff7e2a915ac8bf8150885b2aac2fdff2f14"
DEVELOPMENT_WIDTHS = (128, 1024, 8192)
RELEASE_RAW_LIMIT = .007
DEVELOPMENT_RAW_LIMIT = .070
DEVELOPMENT_NEAR_TIE = .020
METADATA_MODES = ("metadata", "projection-metadata", "qkvo-metadata", "down-metadata", "full-metadata")
GRAPH_MODES = ("graph", "projection-graph", "qkvo-graph", "down-graph", "full-graph")
QKVO_STAGE_SCHEMA = "airk.private.compact-fp32-qkvo-projection-candidate/1"
QKVO_STAGE_SHA = "dbf3a746b8ec66d81ba68798988cdc4068d99841c538cbb2963a37125bd3fb5b"
QKVO_GRAPH_SHA = "0f2407345000cb244674238eead6178bc711238f2a4ddc5b61eef603d87be0b1"
QKVO_REFERENCE_SHA = "768ef122dff10af1cddb887650d6231f65817a8c7138e4294953cb4a95a044a6"
QKVO_REVIEWED_RECEIPT_SHA = "665de6f08b4d65db6ed8d0b3257c440f88a5d42924640cb4f453a8ff16f386ab"
QKVO_REVIEWED_GRAPH_SHA = "ef46663525337868d8a47fe440b9199bc927cf1625e4b4caa3abeb764e93a1f1"
QKVO_EXPERIMENT_SHA = "355c354af9e55f36e6be41fbfd8b89719e13d5c6d5193cbb8bc03e2359ebd97e"
PROJECTION_METADATA_SCHEMA = "airk.private.compact-fp32-qkv-probe-metadata/1"
PROJECTION_GRAPH_SCHEMA = "airk.private.compact-fp32-qkv-graph-diagnostic/1"
PROJECTION_T32_SCHEMA = "airk.private.compact-fp32-qkv-t32-component/1"
PROJECTION_REFERENCE_SCHEMA = "airk.private.compact-fp32-qkv-t32-reference/1"
PROJECTION_REFERENCE_SHA = "304d9b16922ce88f9db181c1e1956d9c0aa99ab29c5ef4dff26f2c37f5b0fa28"
PROJECTION_REVIEWED_RECEIPT_SHA = "9271976d11b3882f4636badb321cdd817e64c3ce87bfb8197c3c5b1bb67509d8"
PROJECTION_REVIEWED_GRAPH_SHA = "42a273598b5c8678535691426ed725041a5d9565ff1fbaf535a43938894f07a4"
PROJECTION_TOTAL_NODES = 1942
PROJECTION_STAGE_SHA = "87ec206416eda607ff1585809d085e37fc4f8b99a5fc029308b5c78218cefcc2"
PROJECTION_BASELINE_SHA = "971cf2d22d8bb7c874f3e5f5ce014bf2c494d1fb7245126d27d722223ac1577f"
PROJECTION_GRAPH_SHA = "e621cf6bffad1f0a27e3cc1ecba5b4379949295498f6585963620d71cfe4ff14"
PROJECTION_WEIGHTS_SHA = "0c4bd89d032e7dd821071c3559776cd3da50d54d84b6e575373fd8cf7003a7c3"
PROJECTION_WEIGHTS_BYTES = 3_441_397_760
MODEL = "arbiter-context-qwen3-17b"
PREPARE_SHA = "850cf966a73bc5ecb0aa79cc3d22559cdd3ad855fbc38f73e361f136ea88c953"
T32_DIGEST = "ec7b4845c37fb7cbe2a64a866f8b6cdc03479d65593cdb0a1d01102fe56fc335"
METADATA_SCHEMA = "airk.private.compact-fp16-probe-metadata/1"
GRAPH_SCHEMA = "airk.private.compact-fp16-graph-diagnostic/1"
T32_SCHEMA = "airk.private.compact-fp16-t32-component/2"
BOUNDED_SCHEMA = "airk.private.compact-fp16-bounded-t32-component/1"
BOUNDED_POLICY = b"airk.private.compact-fp16-cuda-bounded-hybrid/1"
LAYER0_TAPS = (
    ("/airk_g5/attention/0/q16", 2048),
    ("/airk_g5/attention/0/k16", 1024),
    ("/airk_g5/weight//scorer/layers.0/self_attn/v_proj/MatMul/output16", 1024),
    ("/airk_g5/attention/0/raw16", 2048),
)
LAYER0_PROJECTION_TAPS = (
    ("/airk_g5/weight//scorer/layers.0/self_attn/k_proj/MatMul/input16", 2048),
    ("/airk_g5/weight//scorer/layers.0/self_attn/k_proj/MatMul/output16", 1024),
    ("/airk_g5/weight//scorer/layers.0/self_attn/v_proj/MatMul/output16", 1024),
    ("/airk_g5/weight//scorer/layers.0/self_attn/q_proj/MatMul/output16", 2048),
)
TAP_SCOPES = {
    "layer0-t32": (
        "airk.private.compact-fp16-layer0-taps/1", b"airk.private.compact-fp16-cuda-layer0-taps/1", LAYER0_TAPS,
    ),
    "layer0-projection-t32": (
        "airk.private.compact-fp16-layer0-projection-taps/1",
        b"airk.private.compact-fp16-cuda-layer0-projection-taps/1", LAYER0_PROJECTION_TAPS,
    ),
}
REVIEWED_MODES = ("bounded-t32", *PROJECTION_SCORE_MODES, *TAP_SCOPES)
SCORING_MODES = ("t32", *REVIEWED_MODES)
REVIEWED_RECEIPT_SHA = "6d5e8434a350ef1a1f11decd9c6a38bf374fc11fedffb86878cc738806899d34"
REVIEWED_GRAPH_SHA = "47a7cd6f9e0ddf50911cc7a745bcbec4ca2a963772aad5e2e6acc663e26ec104"
BOUNDED_SOURCE = "crates/airk-retrieval/src/arbiter/runtime/bounded_hybrid.rs"
CPU_VALUES_SOURCE = "crates/airk-retrieval/src/arbiter/runtime/cuda/graph_metadata/cpu_values.rs"
CPU = "CPUExecutionProvider"
BOUNDED_CPU_NODES = 374
BOUNDED_TOTAL_NODES = 1746
INDEPENDENT_SCHEMA = "airk.private.compact-fp16-independent-proof/1"
HOST = 32 << 30
WALL = 120
LOG_CAP = 64 << 20
JOB = {"process_memory_limit_bytes": HOST, "job_memory_limit_bytes": HOST, "flags": 0x2308}
BOUNDS = {
    "schema": "airk.arbiter.gpu-probe-reservation/1", "host_limit_bytes": HOST,
    "device_limit_bytes": 24 << 30, "deadline_ms": 120_000,
}
NAMES = {f"/airk_g5/attention/{index}/efficient" for index in range(28)}
CUDA = "CUDAExecutionProvider"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def core(pin):
    return {key: pin[key] for key in ("path", "sha256")}


def finite(value, low, high):
    return type(value) in (int, float) and math.isfinite(value) and low <= value <= high


def attention_logs(raw, mode="t32"):
    """Accept exact stock ORT backend rows for the single accepted forward."""
    require(0 < len(raw) <= LOG_CAP, "complete bounded native log required")
    rows = {}
    for line in raw.decode("utf-8", errors="replace").splitlines():
        if "SdpaKernel=" not in line:
            continue
        line = line.removeprefix(f"test {SELECTORS[mode]} ... ")
        line = line.removeprefix("\x1b[36m").removesuffix("\x1b[0m")
        match = re.fullmatch(
            r"Operator=GroupQueryAttention Node=(\S+) DataType=fp16 SdpaKernel=FLASH_ATTENTION"
            r"(?: [A-Za-z][A-Za-z0-9]*=\S+)*", line,
        )
        require(match is not None, "foreign or non-Flash native attention backend record")
        name = match.group(1)
        require(name in NAMES and name not in rows, "foreign or duplicate native attention node")
        rows[name] = line
    require(set(rows) == NAMES, "exact 28 named native FP16 FlashAttention records required")
    return list(rows.values())


def job_limits(api, job, memory=HOST):
    """Read the already assigned Job without mutating its original limits."""
    from ctypes import wintypes as w

    class Basic(ctypes.Structure):
        _fields_ = [
            ("process_time", ctypes.c_longlong), ("job_time", ctypes.c_longlong),
            ("flags", w.DWORD), ("minimum", ctypes.c_size_t), ("maximum", ctypes.c_size_t),
            ("active_limit", w.DWORD), ("affinity", ctypes.c_size_t),
            ("priority", w.DWORD), ("scheduling", w.DWORD),
        ]

    class Limits(ctypes.Structure):
        _fields_ = [
            ("basic", Basic), ("io", ctypes.c_ulonglong * 6),
            ("process", ctypes.c_size_t), ("job", ctypes.c_size_t),
            ("peak_process", ctypes.c_size_t), ("peak_job", ctypes.c_size_t),
        ]

    observed = Limits()
    if not api.QueryInformationJobObject(job, 9, ctypes.byref(observed), ctypes.sizeof(observed), None):
        raise ctypes.WinError(ctypes.get_last_error())
    actual = {"process_memory_limit_bytes": observed.process, "job_memory_limit_bytes": observed.job,
              "flags": observed.basic.flags}
    expected=dict(JOB,process_memory_limit_bytes=memory,job_memory_limit_bytes=memory)
    require(actual == expected and observed.basic.active_limit == 2, "actual suspended Job limits differ")
    return actual


def observed_native_exit(owned):
    if owned["timed_out"] is True:
        raise TimeoutError("native child exceeded the original Compact phase deadline")
    require(owned["log_oversized"] is False, "actual stdout exceeded its retained bound")
    close = owned["handle_close_proof"]
    require(owned["owned_descendants_settled"] is True
            and owned["owned_job_forced_termination"] is False
            and owned["owned_job_final_active_processes"] == 0
            and owned["owned_job_final_member_pids"] == []
            and close["forced"] is False and close["settled"] is True
            and close["final_active"] == 0 and close["final_member_pids"] == [],
            "native child Job did not settle naturally within the owned 32 GiB Job")
    status = owned["exit_code"]
    require(type(status) is int and 0 <= status <= 0xFFFFFFFF, "invalid native Windows exit status")
    if status == 0xC0000005:
        leader = {"pid": owned["owned_leader_pid"], "exit_observed": True, "exit_code": status}
        require(leader in close["retained_exits"], "native access violation exit was not observed at handle close")
        raise ValueError("native child exited with Windows access violation (0xC0000005)")
    return status


def exact_pin(path, expected, base):
    value = base.pin(path)
    require(value["sha256"] == expected, f"reviewed digest differs: {path}")
    return value


def retained_core(pin, base):
    path = Path(pin["path"])
    value = exact_pin(path, pin["sha256"], base)
    require(core(value) == pin, "native core file pin differs")
    return value


def retained_cargo_file(original, record, build, base, audit):
    require(record["path"] == str(original) and re.fullmatch("[a-f0-9]{64}", record["sha256"]),
            "record does not bind the original selected Cargo path")
    copy = Path(build["path"]).parent / "retained-cargo-lineage" / (record["sha256"] + ".cargo")
    audit([copy])
    pin = exact_pin(copy, record["sha256"], base)
    require(pin["bytes"] == record["bytes"], "retained Cargo bytes differ")
    return pin


def retained_cargo_chain(path, digest, build, image, manifest, base, audit, parent):
    require(path.is_absolute() and path.name == "execution.json" and path.parent.parent == CAMPAIGN,
            "prior direct campaign closure required")
    audit([path])
    pin = exact_pin(path, digest, base)
    previous = base.read(pin["path"])
    custody = previous["owned"]["suspended_assignment"]
    require(previous["schema"] == "airk.private.owned-compact-fp16-probe/1"
            and previous["complete"] is True and previous["failure"] is None
            and previous["whole_phase_expired"] is False and parent.settled(previous["owned"])
            and previous["build"] == build and previous["image"] == image
            and previous["runtime_manifest"] == manifest and "retained_cargo_proof" not in previous
            and custody["image"] == core(image) and custody["execution_user_sid"] == custody["default_owner_sid"] == base.SID
            and len(previous["compile_time_cuda_approvals"]) == 3,
            "prior observed original Cargo image, native custody or lineage differs")
    return pin, previous


def linked_image(build, image, base, audit, mode, *, source_snapshot=None, cargo_chain=None):
    observed = base.read(build["path"], 32 << 20)
    command = observed["command"]
    require(observed["schema"] == "airk-readiness-command/1" and observed["passed"] is True
            and observed["inputs_unchanged"] is True and observed["exit_code"] == 0
            and "test" in command and "airk-inference" in command and "--no-run" in command
            and "--all-features" in command and "--message-format=json" in command,
            "successful stable scoped Cargo JSON image build required")
    leaf = observed["log"]
    require(Path(leaf).name == leaf and leaf not in ("", ".", ".."), "Cargo log sibling leaf required")
    log = Path(build["path"]).with_name(leaf)
    audit([log])
    log_pin = base.pin(log)
    require(0 < log_pin["bytes"] <= 32 << 20, "bounded actual Cargo log required")
    selected = []
    with log.open(encoding="utf-8") as stream:
        for line in stream:
            if line.startswith("{"):
                event = json.loads(line)
                if (event.get("reason") == "compiler-artifact" and event["target"]["name"] == "airk_inference"
                        and event["profile"]["test"] is True and event.get("executable")):
                    selected.append(Path(event["executable"]))
    require(len(selected) == 1, "exactly one compiler-emitted Inference test image required")
    if cargo_chain is None:
        audit(selected)
        artifact = base.pin(selected[0])
    else:
        require(cargo_chain["build_log"] == log_pin, "original build log changed")
        artifact = retained_cargo_file(selected[0], cargo_chain["linked_image"], build, base, audit)
    require((artifact["sha256"], artifact["bytes"]) == (image["sha256"], image["bytes"]),
            "staged image differs from the linked Cargo image")
    native = observed["groups"]["native"]["files"]
    inference = "crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2"
    relevant = (
        inference + "/cuda_qualification.rs", inference + "/cuda_qualification/fp16_compact.rs",
        inference + "/cuda_qualification/CONTRACT.md",
        "crates/airk-retrieval/src/arbiter/runtime.rs",
        "crates/airk-retrieval/src/arbiter/runtime/cuda.rs",
        "crates/airk-retrieval/src/arbiter/runtime/cuda/fp16.rs",
        "crates/airk-retrieval/src/arbiter/runtime/cuda/diagnostic.rs",
    )
    if mode in REVIEWED_MODES:
        relevant += (BOUNDED_SOURCE, inference + "/cuda_qualification/hybrid_profile_trace.rs")
    if mode in TAP_SCOPES or mode in ("projection-graph", "qkvo-graph", "down-graph", "full-graph", *PROJECTION_SCORE_MODES):
        relevant += ("crates/airk-retrieval/src/arbiter/runtime/bounded_hybrid/layer0.rs",)
    if mode in TAP_SCOPES or mode in WIDE_CAPTURES:
        relevant += ("crates/airk-retrieval/src/arbiter/runtime/cuda/run.rs",)
    if mode == FULL_CUT:
        relevant += ("crates/airk-retrieval/src/arbiter/runtime/bounded_hybrid/layer0/full_cut.rs",
                     "crates/airk-retrieval/src/arbiter/runtime/cuda/run.rs")
    if mode in FULL_MODES:
        relevant += (inference + "/cuda_qualification/fp16_compact/full.rs",)
    if mode in DOWN_MODES:
        relevant += (inference + "/cuda_qualification/fp16_compact/down.rs",)
    if mode in (OBSERVED_MODE, DOWN_SCORE, *FULL_OBSERVED):
        relevant += (inference + "/cuda_qualification/fp16_compact/observed.rs",)
    if mode == CAL_MODE:
        if CPU_VALUES_SOURCE in native:
            relevant += (CPU_VALUES_SOURCE,)
        registration = "crates/airk-retrieval/src/arbiter/scientific_cal.rs"
        if registration in native:
            relevant += (registration,)
        relevant += (inference + "/cuda_qualification/fp16_compact/cal.rs", inference + "/cuda_qualification/fp16_compact/observed.rs")
    if mode in TIMING_MODES:
        relevant += (inference + "/cuda_qualification/fp16_compact/timing.rs",)
    if source_snapshot is None:
        paths = {relative: ROOT / relative for relative in relevant}
    else:
        expected = Path(build["path"]).resolve(strict=True).parent / "retained-native-sources"
        require(source_snapshot.resolve(strict=True) == source_snapshot == expected,
                "source snapshot must be the sealed Cargo receipt's retained-native-sources directory")
        paths = {relative: source_snapshot / (native[relative] + ".source") for relative in relevant}
        audit([source_snapshot, *paths.values()])
    sources = {relative: exact_pin(path, native[relative], base) for relative, path in paths.items()}
    return log_pin, artifact, sources


def decision_build_approvals(log, artifact, manifest, base, audit, *, cargo_chain=None):
    """Bind rustc's exact environment dependencies to the selected Cargo executable."""
    image = Path(artifact["path"] if cargo_chain is None else cargo_chain["linked_image"]["path"])
    deps = image.parent
    require(deps.name == "deps" and image.stem.startswith("airk_inference-"), "Cargo deps image required")
    fingerprints = deps.parent / ".fingerprint"
    inf = fingerprints / image.stem.replace("airk_inference-", "airk-inference-", 1) / "test-lib-airk_inference.json"
    with Path(log["path"]).open(encoding="utf-8") as stream:
        events = (json.loads(line) for line in stream if line.startswith("{"))
        libs = [Path(name) for event in events if event.get("reason") == "compiler-artifact"
                and event["target"]["name"] == "airk_retrieval" and event["profile"]["test"] is False
                for name in event["filenames"] if name.endswith(".rlib")]
    require(len(libs) == 1 and libs[0].parent == deps and libs[0].stem.startswith("libairk_retrieval-"),
            "exact Retrieval rlib required")
    key = libs[0].stem.removeprefix("libairk_retrieval-")
    fingerprint = fingerprints / ("airk-retrieval-" + key) / "lib-airk_retrieval"
    info = deps / ("airk_retrieval-" + key + ".d")
    if cargo_chain is None:
        audit([inf, fingerprint, info])
        pins = [base.pin(path) for path in (inf, fingerprint, info)]
    else:
        pins = [retained_cargo_file(original, record, cargo_chain["build"], base, audit)
                for original, record in zip((inf, fingerprint, info), cargo_chain["compile_time_cuda_approvals"], strict=True)]
        inf, fingerprint, info = (Path(pin["path"]) for pin in pins)
    lineage = [row[3] for row in base.read(str(inf))["deps"] if row[1] == "airk_retrieval"]
    require(lineage == [int.from_bytes(bytes.fromhex(fingerprint.read_text().strip()), "little")],
            "Inference test image did not link selected Retrieval fingerprint")
    expected = {"# env-dep:AIRK_APPROVED_CUDA_MANIFEST_SHA256=" + manifest["sha256"],
                "# env-dep:AIRK_STATIC_CUDA_BUILD_SHA256=" + base.read(manifest["path"])["static_core_build"]}
    require(expected <= set(info.read_text(encoding="utf8").splitlines()), "compiled CUDA approval pins missing")
    return pins


def has_output_projection(projection):
    return projection is not None and projection[1]["schema"] == QKVO_STAGE_SCHEMA


def projection_schema(projection, qkv_schema):
    if projection is not None and projection[1]["schema"] == FULL_STAGE_SCHEMA:
        return qkv_schema.replace("fp32-qkv-", "fp32-full-")
    if projection is not None and projection[1]["schema"] == DOWN_STAGE_SCHEMA:
        return qkv_schema.replace("fp32-qkv-", "fp32-qkv-down-")
    return qkv_schema.replace("fp32-qkv-", "fp32-qkvo-") if has_output_projection(projection) else qkv_schema


def projection_stage(path, expected, base, audit, *, output_projection=False, down=False, full=False):
    stage_sha = QKVO_STAGE_SHA if output_projection else PROJECTION_STAGE_SHA
    graph_sha = QKVO_GRAPH_SHA if output_projection else PROJECTION_GRAPH_SHA
    stage_schema = (QKVO_STAGE_SCHEMA if output_projection
                    else "airk.private.compact-fp32-qkv-projection-candidate/1")
    count = 112 if output_projection or down else 84
    if full:
        stage_sha, graph_sha, stage_schema, count = FULL_STAGE_SHA, FULL_GRAPH_SHA, FULL_STAGE_SCHEMA, 196
    if down:
        stage_sha, graph_sha, stage_schema = DOWN_STAGE_SHA, DOWN_GRAPH_SHA, DOWN_STAGE_SCHEMA
    require(path.is_absolute() and path.parent.parent == CAMPAIGN and path.name == "stage.json"
            and expected == stage_sha, "exact retained Compact projection stage required")
    audit([path])
    pin = exact_pin(path, expected, base)
    value = base.read(pin["path"], 1 << 20)
    require(value["schema"] == stage_schema
            and value["complete"] is True
            and value["qualification"] is value["product_admission"] is False
            and all(type(value[key]) is int and value[key] == 0
                    for key in ("native_load_attempts", "native_forward_attempts"))
            and value["graph_proof"]["projection_count"] == count
            and (value["graph_proof"]["prior_qkv_routes"] == 84 and value["graph_proof"]["new_input_and_weight_casts"] == 56
                 and value["graph_proof"]["removed_half_output_casts"] == 28 and value["graph_proof"]["source_graph_recovered_byte_for_byte"] is True
                 if down else full_stage_proof(value["graph_proof"]) if full else value["graph_proof"]["added_cast_count"] == 3 * count),
            "reviewed Compact projection stage scope differs")
    if output_projection:
        require(value["projection_experiment"]["sha256"] == QKVO_EXPERIMENT_SHA,
                "reviewed standalone Compact output projection experiment required")
    graph, weights = value["candidate_graph"], value["candidate_weights"]
    require(set(graph) == set(weights) == {"path", "bytes", "sha256"}
            and graph["path"] == str(path.with_name("model.onnx"))
            and graph["sha256"] == graph_sha and type(graph["bytes"]) is int
            and 0 < graph["bytes"] <= 16 << 20
            and weights == {"path": str(path.with_name("weights.bin")),
                            "bytes": PROJECTION_WEIGHTS_BYTES, "sha256": PROJECTION_WEIGHTS_SHA}
            and value["baseline_graph"]["sha256"] == PROJECTION_BASELINE_SHA
            and value["baseline_weights"]["sha256"] == weights["sha256"]
            and value["baseline_weights"]["bytes"] == weights["bytes"],
            "reviewed Compact Q/K/V graph or same-half sidecar identity differs")
    audit([Path(graph["path"]), Path(weights["path"])])
    base.pinned(graph)
    base.pinned(weights)
    return pin, value


def full_stage_proof(proof):
    return (proof["fp32_weight_upcasts"] == 196 and proof["removed_activation_and_output_half_boundaries"] == 392
            and proof["attention_nodes"] == 28 and proof["source_graph_recovered_byte_for_byte"] is True
            and proof["retained_sidecar"] is True)


def validate_projection_baseline(stage, independent):
    require(independent["candidate"] == {"graph": stage["baseline_graph"], "weights": stage["baseline_weights"]},
            "Q/K/V projection stage does not bind the original independent baseline")


def read_projection_reference(path, expected, projection, independent, base, audit):
    output_projection = has_output_projection(projection)
    tag = "qkvo" if output_projection else "qkv"
    reference_sha = QKVO_REFERENCE_SHA if output_projection else PROJECTION_REFERENCE_SHA
    require(path.is_absolute() and path.parent.parent == CAMPAIGN
            and path.name == f"compact-{tag}-fp32-t32-reference.json"
            and expected == reference_sha, "exact retained Compact projection CPU proof required")
    audit([path])
    pin = exact_pin(path, expected, base)
    proof = base.read(pin["path"], 16 << 20)
    stage_pin, stage = projection
    require(proof["schema"] == projection_schema(projection, PROJECTION_REFERENCE_SCHEMA) and proof["model"] == MODEL
            and proof["complete"] is proof["model_inference"] is True
            and proof["native_execution"] is proof["qualification"] is proof["product_admission"] is False
            and proof["reference_device"] == "cpu" and type(proof["torch_forward_invocations"]) is int
            and proof["torch_forward_invocations"] == 1 and proof["stage_receipt"] == stage_pin
            and proof["candidate"] == {"graph": stage["candidate_graph"], "weights": stage["candidate_weights"]}
            and proof["baseline_candidate"] == independent["candidate"]
            and proof["verification"]["projection_graph"] == stage["graph_proof"],
            "independent Compact Q/K/V proof has different stage, assets or forward scope")
    projection_names = ("q", "k", "v", "o") if output_projection else ("q", "k", "v")
    routes = [f"layers.{layer}.self_attn.{name}_proj.weight"
              for layer in range(28) for name in projection_names]
    require(proof["recipe"] == {f"{tag}_fp32_routes": routes, "context_flash_single_tile": True},
            "independent Compact matching projection recipe differs")
    reference = dict(proof["reference"])
    matching = reference.pop(f"matching_{tag}_fp32")
    original = dict(independent["reference"])
    original.pop("matching_fp16")
    require(reference == original and reference["name"] == "diagnostic-all-live-32"
            and reference["input"]["input_digest"] == T32_DIGEST
            and all(isinstance(scores, list) and len(scores) == 8
                    and all(finite(score, -math.inf, math.inf) for score in scores)
                    for scores in (matching, reference["original_fp32"])),
            "independent Compact Q/K/V reference input or finite vectors differ")
    return pin


def read_development_reference(path, expected, projection, independent, base, audit, deadline):
    require(path.is_absolute() and path.parent.parent == CAMPAIGN
            and path.name == "compact-qkv-fp32-wider-reference.json"
            and expected == DEVELOPMENT_REFERENCE_SHA,
            "exact retained Compact wider Q/K/V CPU proof required")
    audit([path])
    pin = exact_pin(path, expected, base)
    proof = base.read(pin["path"], 16 << 20)
    stage_pin, stage = projection
    require(proof["schema"] == DEVELOPMENT_REFERENCE_SCHEMA and proof["model"] == MODEL
            and proof["complete"] is proof["model_inference"] is True
            and proof["native_execution"] is proof["qualification"] is proof["product_admission"] is False
            and proof["reference_device"] == "cpu" and proof["torch_forward_invocations"] == 3
            and proof["stage_receipt"] == stage_pin
            and proof["candidate"] == {"graph": stage["candidate_graph"], "weights": stage["candidate_weights"]}
            and proof["baseline_candidate"] == independent["candidate"]
            and proof["verification"]["projection_graph"] == stage["graph_proof"],
            "independent wider Q/K/V proof has different artifacts or scope")
    routes = [f"layers.{layer}.self_attn.{name}_proj.weight"
              for layer in range(28) for name in ("q", "k", "v")]
    require(proof["recipe"] == {
        "qkv_fp32_routes": routes, "context_flash_single_tile": False, "torch_cpu_threads": 8,
        "wider_attention": {"algorithm": "causal-reverse-tile-fp16-exponent",
                            "query_block": 64, "key_block": 64, "split_kv": False},
    }, "independent wider Q/K/V precision boundaries differ")

    def check():
        require(time.monotonic() < deadline, "original Compact phase deadline expired")

    checker = runpy.run_path(str(ROOT / "scripts/arbiter-fp16-execution/verify_compact.py"))
    originals, _ = checker["wider_references"](check)
    references = proof["references"]
    require(isinstance(references, list) and len(references) == len(DEVELOPMENT_WIDTHS),
            "expected three independent wider reference rows")
    for row, original, width in zip(references, originals, DEVELOPMENT_WIDTHS, strict=True):
        require(set(row) == set(original) | {"matching_qkv_fp32"}
                and all(row[key] == value for key, value in original.items())
                and len(row["input"]["input_ids"]) == width
                and len(row["matching_qkv_fp32"]) == 8
                and all(finite(score, -math.inf, math.inf) for score in row["matching_qkv_fp32"]),
                "wider reference row or exact original FP32 lineage differs")
    return pin, proof


def validate_projection_artifacts(pins, profile, projection, base):
    pin, stage = projection
    source = base.read(pins["source_manifest"]["path"])
    native = base.read(pins["native_input"]["path"])
    require(profile["device_execution"]["graph"] == stage["candidate_graph"]["sha256"]
            and source["schema"] == projection_schema(projection, "airk.private.compact-fp32-qkv-source/1")
            and source["projection_stage"] == core(pin)
            and source["candidate"] == {"graph": stage["candidate_graph"], "weights": stage["candidate_weights"]}
            and native["files"]["model.onnx"] == stage["candidate_graph"]["path"]
            and native["files"]["weights.bin"] == stage["candidate_weights"]["path"],
            "emitted Compact Q/K/V profile or candidate assets differ")


def validate_request(mode, request, request_pin, evidence, manifest, *, projection=None):
    require((projection is not None) == (mode in PROJECTION_MODES), "Q/K/V projection stage scope differs")
    metadata_mode = mode in METADATA_MODES
    require(request["schema"] == "airk.private.cuda-component-probe-request/1"
            and request["kind"] == ("prepare" if metadata_mode else "probe")
            and request["runtime_manifest"] == core(manifest)
            and request["bounds"] == BOUNDS and request["job_limit_bytes"] == HOST
            and (request["cold_bytes"], request["warm_bytes"], request["headroom_bytes"])
                == (16 << 30, 8 << 30, 2 << 30), "reviewed Compact request, runtime or phase bounds differ")
    if metadata_mode:
        expected_prepare = PREPARE_SHA
        if mode == "full-metadata":
            expected_prepare = "71eb0865b26801447283c2c186748bdfa464f341ef792c5a9478b8e8d03c5fa5"
            require(Path(request["runtime_manifest"]["path"]) == CAMPAIGN / "windows-cu15-scientific-provider-stage-r1/airk-cuda-runtime.json"
                    and request["runtime_manifest"]["sha256"] == "9433753580c8a370bbea42119aa67ac81306229bcf6876514213f3e2979083b5",
                    "full diagnostic requires the original CU15 runtime in the D: stage")
        require(request_pin["sha256"] == expected_prepare, "exact reviewed Compact Prepare input required")
        require(evidence["schema"] == INDEPENDENT_SCHEMA and evidence["complete"] is True
                and evidence["model"] == MODEL and evidence["model_inference"] is True
                and evidence["native_execution"] is evidence["qualification"] is False
                and evidence["reference"]["input"]["input_digest"] == T32_DIGEST,
                "reviewed independent Compact candidate/T32 proof required")
    else:
        schema = projection_schema(projection, PROJECTION_METADATA_SCHEMA) if projection is not None else METADATA_SCHEMA
        require(evidence["schema"] == schema and evidence["probe_request"] == core(request_pin)
                and evidence["native_input"] == request["gpu_input"]
                and evidence["runtime_manifest"] == core(manifest), "metadata does not bind this Probe")
        if projection is not None:
            require(evidence.get("projection_stage") == core(projection[0]),
                    "Q/K/V projection metadata stage differs")


def validate_metadata(value, paths, request, evidence, manifest, base, *, projection=None):
    schema = projection_schema(projection, PROJECTION_METADATA_SCHEMA) if projection is not None else METADATA_SCHEMA
    require(value["schema"] == schema and value["request"] == core(request)
            and value["independent_proof"] == core(evidence) and value["runtime_manifest"] == core(manifest)
            and value["execution_user"] == base.SID and value["reference_input_digest"] == T32_DIGEST
            and value["qualified"] is value["model_requests_dispatched"] is value["runtime_admission_dispatched"] is False
            and value["forward_count_observed"] is None and finite(value["elapsed_us"], 0, 120_000_000),
            "native Compact metadata identity or dispatch scope differs")
    directory = paths["metadata"].parent
    leaves = {"source_manifest": "source-manifest.json", "profile_pin": "profile.json",
              "native_input": "native-input.json", "probe_request": "probe-request.json",
              "calibration": Path(value["calibration"]["path"]).name}
    require(leaves["calibration"] not in ("", ".", "..") and len(set(leaves.values())) == len(leaves)
            and {path.name for path in directory.iterdir()} == {*leaves.values(), paths["metadata"].name},
            "fresh native metadata file roster differs")
    pins = {}
    for key, leaf in leaves.items():
        require(value[key]["path"] == str(directory / leaf), "native metadata output escaped the checked directory")
        pins[key] = retained_core(value[key], base)
    profile = base.read(pins["profile_pin"]["path"])
    request = base.read(pins["probe_request"]["path"])
    require(profile["model"] == MODEL and profile["manifest"] == value["profile"]
            and profile["device_execution"]["precision"] == "float16"
            and request["gpu_input"] == core(pins["native_input"])
            and request["bounds"] == BOUNDS and request["runtime_manifest"] == core(manifest)
            and request["kind"] == "probe" and request["job_limit_bytes"] == HOST
            and (request["cold_bytes"], request["warm_bytes"], request["headroom_bytes"])
                == (16 << 30, 8 << 30, 2 << 30),
            "emitted Compact FP16 profile or Probe shape differs")
    if projection is not None:
        require(value.get("projection_stage") == core(projection[0]), "Q/K/V projection metadata stage differs")
        validate_projection_artifacts(pins, profile, projection, base)
    return pins


def validate_t32_comparisons(row):
    require(finite(row["matching_fp16_raw_error"], 0, RELEASE_RAW_LIMIT)
            and finite(row["matching_fp16_mass_error"], 0, .05)
            and row["matching_fp16_argmax_matches"] is True,
            "native T32 matching FP16 comparison refused")
    require(finite(row["original_fp32_mass_error"], 0, .05)
            and finite(row["original_fp32_raw_error"], 0, math.inf)
            and type(row["original_fp32_argmax_matches"]) is bool,
            "native T32 original FP32 mass or diagnostic evidence refused")


def validate_native_identity(value, schema, owned, request, evidence, metadata, manifest, image, base):
    require(value["schema"] == schema and value["complete"] is True
            and value["qualified"] is value["product_admission"] is value["native_backend_attested"] is False
            and value.get("error") is None and value["request"] == core(request)
            and value["runtime_manifest"] == core(manifest) and value["profile"] == metadata["profile"]
            and value["native_input"] == metadata["native_input"] and value["source_manifest"] == metadata["source_manifest"]
            and value["metadata_receipt"] == core(evidence) and value["independent_proof"] == metadata["independent_proof"]
            and value["pid"] == owned["owned_leader_pid"]
            and value["native_binary_sha256"] == image["sha256"]
            and value["execution_user"] == value["default_owner"] == base.SID,
            "native Compact execution identity or scope differs")
    require(value["actual_job"] == JOB and value["bounds"] == BOUNDS
            and value["native_host_reservation_bytes"] == HOST
            and finite(value["peak_process_rss_bytes"], 1, HOST)
            and value["session_teardown_observed"] is value["device_teardown_observed"] is True
            and value["measured_peak_device_bytes"] is None
            and finite(value["elapsed_us_to_final_report_start"], 0, 119_999_999)
            and value["native_control_cleanup_observed"] is value["native_host_charge_release_observed"] is True
            and value["process_cuda_poisoned"] is value["native_control_retained_for_unknown_cuda"] is False
            and value["native_control_cleanup_error"] is None,
            "native Compact Job, original host charge or synchronized settlement differs")
    before, after = value["adapter_before"], value["adapter_after"]
    fixed = ("uuid", "ordinal", "compute_major", "compute_minor", "driver_api_version",
             "driver_image_sha256", "physical_total_bytes")
    require(len(before["uuid"]) == 16 and any(before["uuid"])
            and all(before[key] == after[key] for key in fixed)
            and all(0 <= row["free_bytes"] <= row["total_bytes"] <= row["physical_total_bytes"]
                    and row["total_bytes"] > 0 for row in (before, after)),
            "actual matching adapter observations required")


def read_operations(path):
    with path.open("rb") as source:
        raw = source.read((128 << 10) + 1)
    require(0 < len(raw) <= 128 << 10 and raw.endswith(b"\n"), "complete bounded operation journal required")
    return [json.loads(line) for line in raw.splitlines()]


def validate_graph(value, events, component):
    require(type(value["numeric_forward_attempts"]) is int and value["numeric_forward_attempts"] == 0
            and value["native_forward_returned"] is value["forward_synchronized"] is False
            and value["rows"] == value["input_refusals"] == [] and value["runtime_profile"] is None,
            "graph diagnostic performed scorer work")
    require(value.get("final_report_retention_exceeded_original_deadline", False) is False
            and finite(value["load_us"], 0, 119_999_999)
            and finite(value["load_native_call_us"], 0, value["load_us"])
            and value["load_us"] <= value["elapsed_us_to_final_report_start"],
            "bounded graph Load timing required")
    snapshot = value["graph_observation"]
    require(isinstance(snapshot, dict) and snapshot["session_created"] is True
            and isinstance(snapshot["logs"], dict),
            "graph diagnostic did not create a disposable session")
    teardown = snapshot["teardown"]
    require(isinstance(teardown, dict)
            and all(teardown[key] is True for key in ("pre_sync_observed", "session_drop_attempted",
                                                     "session_drop_returned", "post_sync_observed"))
            and type(teardown["pending_claims"]) is int and teardown["pending_claims"] == 0
            and teardown["poisoned"] is False and teardown["errors"] == [],
            "graph diagnostic disposable session did not settle")
    place = snapshot["placement"]
    require(isinstance(place, dict)
            and place["scope"] == "onnxruntime-final-session-state-ep-placement"
            and place["complete"] is True
            and all(type(place[key]) is int and place[key] == 0 for key in ("shim_status", "ort_error_code"))
            and isinstance(place["nodes"], list) and 0 < len(place["nodes"]) <= 65_536
            and type(place["reported_nodes"]) is int and place["reported_nodes"] == len(place["nodes"]),
            "complete final diagnostic placement required")
    graph = snapshot["graph_metadata"]
    component["validate_graph_metadata"](graph, place)
    # The shared graph validator independently bounds and cross-maps every placement span.
    placed_bytes = sum(len(node[key].encode("utf-8")) for node in place["nodes"]
                       for key in ("name", "domain", "op", "ep"))
    require(type(place["reported_string_bytes"]) is int and placed_bytes == place["reported_string_bytes"],
            "diagnostic placement byte coverage differs")
    component["validate_operations"](events, value, load_only=True)
    require(events[-1]["native_call_elapsed_us"] == value["load_native_call_us"],
            "native graph Load duration and independent returned marker differ")


def validate_full_routes(nodes, dimensions, cuda_node, typed_values, one_cast_source):
    sites = {
        "self_attn/k_proj": ("input_layernorm/Mul_1_output_0", [2048, 1024]),
        "self_attn/q_proj": ("input_layernorm/Mul_1_output_0", [2048, 2048]),
        "self_attn/v_proj": ("input_layernorm/Mul_1_output_0", [2048, 1024]),
        "self_attn/o_proj": ("self_attn/Reshape_7_output_0", [2048, 2048]),
        "mlp/gate_proj": ("post_attention_layernorm/Mul_1_output_0", [2048, 6144]),
        "mlp/up_proj": ("post_attention_layernorm/Mul_1_output_0", [2048, 6144]),
        "mlp/down_proj": ("mlp/Mul_output_0", [6144, 2048]),
    }
    names = {row["name"]: idx for idx, row in nodes.items()}
    for layer in range(28):
        root = f"/scorer/layers.{layer}/"
        for site, (source, shape) in sites.items():
            name = root + site + "/MatMul"
            idx = names[name]
            cuda_node(idx, "MatMul", name)
            activation, weight = typed_values(idx, 0, (1, 1), name)
            product, = typed_values(idx, 2, (1,), name)
            original, cast = one_cast_source(weight, 10, name)
            require(activation["name"] == root + source and product["name"] == name + "_output_0"
                    and cast == f"/airk_compact_full_projection/layers.{layer}/{site}/weight32"
                    and original["provenance"] == "immutable_session_initializer" and original["rank"] == 2
                    and [dimensions[(original["value"], axis)] for axis in (0, 1)] == shape,
                    f"full FP32 projection activation, weight storage or product differs: {name}")
    return {"cuda_fp32_matmuls": 196, "weight_upcasts": 196, "activation_upcasts": 0, "output_downcasts": 0}


def validate_projection_graph(graph, *, output_projection=False, down=False, full=False, large_qkv=False):
    """Prove the widened projection arithmetic in the validated final SessionState graph."""
    rows = [row for row in graph["rows"] if row["graph"] == 0]
    nodes = {row["node"]: row for row in rows if row["kind"] == "node"}
    values = {row["value"]: row for row in rows if row["kind"] == "value"}
    dimensions = {(row["value"], row["index"]): row["number"] for row in rows if row["kind"] == "dimension"}
    edges, consumers = {}, {}
    for row in rows:
        if row["kind"] != "edge" or row["node"] is None:
            continue
        edges.setdefault((row["node"], row["role"]), []).append(row["value"])
        if row["role"] in (0, 1):
            consumers.setdefault(row["value"], []).append(row["node"])

    require(not large_qkv or not (output_projection or down or full),
            "Large projection diagnostic requires its Q/K/V-only candidate")
    layers, hidden, qwidth, stem_prefix = ((36, 2560, 4096, "/airk_large_qkv_fp32") if large_qkv
                                          else (28, 2048, 2048, "/airk_compact_qkv_fp32"))
    widths = {"k": 1024, "v": 1024, "q": qwidth}
    if output_projection:
        widths["o"] = 2048
    roster = {f"/scorer/layers.{layer}/self_attn/{name}_proj/MatMul": (layer, width, name)
              for layer in range(layers) for name, width in widths.items()}
    suffixes = tuple(f"/self_attn/{name}_proj/MatMul" for name in widths)
    projections = [row for row in nodes.values() if row["name"].endswith(suffixes)]
    require(len(projections) == len(roster) and {row["name"] for row in projections} == set(roster),
            "exact named root projections for the selected recipe required")

    def cuda_node(index, op, name):
        row = nodes.get(index)
        require(row is not None and row["op"] == op and row["domain"] == "" and row["ep"] == CUDA,
                f"Q/K/V projection requires CUDA {op}: {name}")
        return row

    def typed_values(index, role, types, name):
        tensors = [values.get(value) for value in edges.get((index, role), ())]
        require(len(tensors) == len(types)
                and all(row is not None and row["type_state"] == "tensor" and row["element_type"] == dtype
                        for row, dtype in zip(tensors, types)),
                f"Q/K/V projection typed edge differs: {name}")
        return tensors

    def one_cast_source(output, dtype, name):
        node = cuda_node(output["producer"], "Cast", name)
        source, = typed_values(node["node"], 0, (dtype,), name)
        output_row, = typed_values(node["node"], 2, (output["element_type"],), name)
        require(output["value"] == output_row["value"], f"Q/K/V cast output edge differs: {name}")
        return source, node["name"]

    if full:
        return validate_full_routes(nodes, dimensions, cuda_node, typed_values, one_cast_source)

    input_casts = set()
    for row in projections:
        name, index = row["name"], row["node"]
        layer, width, kind = roster[name]
        cuda_node(index, "MatMul", name)
        a32, b32 = typed_values(index, 0, (1, 1), name)
        c32, = typed_values(index, 2, (1,), name)
        stem = stem_prefix + name
        a16, a_cast = one_cast_source(a32, 10, name)
        b16, b_cast = one_cast_source(b32, 10, name)
        require(b_cast == stem + "/weight32" and b16["provenance"] == "immutable_session_initializer"
                and b16["rank"] == 2 and [dimensions.get((b16["value"], axis)) for axis in (0, 1)] == [hidden, width],
                f"compressed weight cast differs: {name}")
        if kind == "o":
            # A binary16 value survives the original round trip exactly. ORT may
            # retain all three Casts or use a single Cast on the GQA result.
            attention_stem = f"/airk_g5/attention/{layer}"
            restored = attention_stem + "/output32"
            raw = a16
            if a16["name"] != attention_stem + "/raw16":
                normal, narrow = one_cast_source(a16, 1, name)
                raw, original_cast = one_cast_source(normal, 10, name)
                require(narrow == "/airk_g5/weight/" + name + "/input16" and original_cast == restored,
                        f"original output projection activation round trip differs: {name}")
            source = nodes.get(raw["producer"])
            require(a_cast in (stem + "/input32", restored) and raw["name"] == attention_stem + "/raw16"
                    and source is not None and source["name"] == attention_stem + "/efficient"
                    and source["domain"] == "com.microsoft" and source["op"] == "GroupQueryAttention"
                    and source["ep"] == CUDA, f"output projection must widen the CUDA attention result: {name}")
        else:
            # ORT can share the activation Cast pair across Q, K and V within a layer.
            activation_stems = {f"/scorer/layers.{layer}/self_attn/{p}_proj/MatMul" for p in ("k", "v", "q")}
            normal, half_cast = one_cast_source(a16, 1, name)
            require(a_cast in {stem_prefix + label + "/input32" for label in activation_stems}
                    and half_cast in {"/airk_g5/weight/" + label + "/input16" for label in activation_stems}
                    and normal["name"] == f"/scorer/layers.{layer}/input_layernorm/Mul_1_output_0",
                    f"Q/K/V original activation narrowing differs: {name}")
        output_users = consumers.get(c32["value"], ())
        require(len(output_users) == 1, f"Q/K/V FP32 result must narrow once: {name}")
        output = cuda_node(output_users[0], "Cast", name)
        c16, = typed_values(output["node"], 2, (10,), name)
        require(output["name"] == stem + "/output16"
                and c16["name"] == "/airk_g5/weight/" + name + "/output16",
                f"Q/K/V original half output binding differs: {name}")
        input_casts.add(a_cast)
    result = {"cuda_fp32_matmuls": len(projections), "weight_upcasts": len(projections),
              "activation_upcasts": len(input_casts), "output_downcasts": len(projections)}
    if down:
        names = {node["name"]: idx for idx, node in nodes.items()}
        for layer in range(28):
            name = f"/scorer/layers.{layer}/mlp/down_proj/MatMul"
            index = names[name]
            cuda_node(index, "MatMul", name)
            activation, weight = typed_values(index, 0, (1, 1), name)
            product, = typed_values(index, 2, (1,), name)
            prefix = f"/airk_compact_mlp_down_fp32/layers.{layer}/"
            half_activation, activation_cast = one_cast_source(activation, 10, name)
            half_weight, weight_cast = one_cast_source(weight, 10, name)
            require(activation_cast == prefix+"input32" and weight_cast == prefix+"weight32"
                    and half_activation["name"] == "/airk_g5/weight/"+name+"/input16"
                    and half_weight["provenance"] == "immutable_session_initializer"
                    and [dimensions[(half_weight["value"], axis)] for axis in (0,1)] == [6144,2048]
                    and product["name"] == name+"_output_0", f"down projection or storage differs at layer {layer}")
            next_nodes = consumers[product["value"]]
            require(len(next_nodes) == 1 and cuda_node(next_nodes[0], "Add", name)["name"] == f"/scorer/layers.{layer}/Add_1",
                    "FP32 down projection must feed the original residual addition")
        result["cuda_fp32_down_projections"] = 28
        result["down_projection_fp32_residual_inputs"] = 28
    return result


def reviewed_graph(receipt, request, evidence, metadata, manifest, parent, component, base, audit, *, projection=None):
    historical = base.read(receipt["path"])
    owned = historical["owned"]
    down = projection is not None and projection[1]["schema"] == DOWN_STAGE_SCHEMA
    full = projection is not None and projection[1]["schema"] == FULL_STAGE_SCHEMA
    mode = "full-graph" if full else "down-graph" if down else "qkvo-graph" if has_output_projection(projection) else "projection-graph" if projection is not None else "graph"
    require(historical["schema"] == "airk.private.owned-compact-fp16-probe/1"
            and historical["mode"] == mode and historical["complete"] is True
            and historical["qualification"] is False and historical["failure"] is None
            and historical["whole_phase_expired"] is False and historical["wall_seconds"] == WALL
            and historical["execution_user_sid"] == base.SID and parent.settled(owned)
            and historical["request"] == request and historical["reviewed_evidence"] == evidence
            and historical["runtime_manifest"] == manifest,
            "reviewed owned graph does not bind this Probe, metadata, runtime and settled Job")
    source = historical["outputs"]["native"]
    operations = historical["outputs"]["operations"]
    expected = (FULL_NATIVE_SHA if full else DOWN_NATIVE_SHA if down else QKVO_REVIEWED_GRAPH_SHA if has_output_projection(projection) else
                PROJECTION_REVIEWED_GRAPH_SHA if projection is not None else REVIEWED_GRAPH_SHA)
    require(source["sha256"] == expected
            and 0 < source["bytes"] <= component["DIAGNOSTIC_PROOFS"]["native_result"]
            and 0 < operations["bytes"] <= 128 << 10,
            "reviewed graph receipt does not authorize the exact native graph")
    directory = Path(receipt["path"]).parent
    require(Path(source["path"]) == directory / "native-result.json"
            and Path(operations["path"]) == directory / "native-result.operations.jsonl",
            "reviewed graph evidence escaped its retained output directory")
    audit([Path(source["path"]), Path(operations["path"])])
    graph = base.read(base.pinned(source))
    events = read_operations(base.pinned(operations))
    schema = projection_schema(projection, PROJECTION_GRAPH_SCHEMA) if projection is not None else GRAPH_SCHEMA
    validate_native_identity(graph, schema, owned, request, evidence, metadata, manifest,
                             historical["image"], base)
    validate_graph(graph, events, component)
    if projection is not None:
        pin, stage = projection
        typed = validate_projection_graph(graph["graph_observation"]["graph_metadata"],
                                          output_projection=has_output_projection(projection), down=down, full=full)
        require(graph.get("projection_stage") == historical.get("projection_stage") == core(pin)
                and historical["projection_assets"] == {name: stage[name] for name in ("candidate_graph", "candidate_weights")}
                and typed == historical["projection_graph"], "reviewed Q/K/V graph stage or typed summary differs")
    placement = graph["graph_observation"]["placement"]
    roster = placement_roster(placement)
    total = (placement["reported_nodes"] if has_output_projection(projection) or down or full else
             PROJECTION_TOTAL_NODES if projection is not None else BOUNDED_TOTAL_NODES)
    require(len(roster) == total and sum(row[-1] == CPU for row in roster) == BOUNDED_CPU_NODES
            and all(row[0] == 0 and row[-1] in (CPU, CUDA) for row in roster),
            "reviewed bounded graph root, node or provider census differs")
    return source, operations, placement


def placement_roster(placement):
    nodes = placement["nodes"]
    require(isinstance(nodes, list) and all(type(node["subgraph_index"]) is int for node in nodes),
            "untyped final placement roster")
    return sorted((node["subgraph_index"], node["name"], node["domain"], node["op"], node["ep"])
                  for node in nodes)


def scientific_validator_source(sources):
    original = sources[BOUNDED_SOURCE]
    source = Path(original["path"]).read_bytes()
    require(hashlib.sha256(source).hexdigest() == original["sha256"], "compiled graph validator source differs")
    if b"airk.private.cuda.bounded-hybrid-validator/2" not in source:
        return original
    child = sources.get(CPU_VALUES_SOURCE)
    require(child is not None, "split validator lacks compiled CPU value checks")
    cpu = Path(child["path"]).read_bytes()
    require(hashlib.sha256(cpu).hexdigest() == child["sha256"], "compiled CPU value validator differs")
    digest = hashlib.sha256(b"AIRK-DIGEST-1\0")
    for field in (b"airk.private.cuda.bounded-hybrid-validator/2", source, cpu):
        digest.update(len(field).to_bytes(8, "big"))
        digest.update(field)
    return {"sha256": digest.hexdigest()}


def bounded_policy_identity(graph_sha, profile_sha, source_sha, *, domain=BOUNDED_POLICY):
    fields = (domain,
              bytes.fromhex(graph_sha), bytes.fromhex(profile_sha), bytes.fromhex(source_sha))
    digest = hashlib.sha256(b"AIRK-DIGEST-1\0")
    for field in fields:
        digest.update(len(field).to_bytes(8, "big"))
        digest.update(field)
    return digest.hexdigest()


def validate_bounded_t32(value, reviewed, placement, source, *, policy=BOUNDED_POLICY, total=BOUNDED_TOTAL_NODES,
                         tokens=32):
    require(value["reviewed_graph"] == core(reviewed)
            and value["bounded_policy_identity"] == bounded_policy_identity(
                reviewed["sha256"], value["profile"], source["sha256"], domain=policy),
            "reviewed graph or bounded FP16 policy identity differs")
    require(placement_roster(value["final_placement"]) == placement_roster(placement),
            "bounded FP16 final placement differs from the complete reviewed graph")
    profiled = value["bounded_profile"]
    expected = {"tokens": tokens, "node_count": total, "cpu_nodes": BOUNDED_CPU_NODES}
    bounds = {"model_run_us": (1, value["rows"][0]["native_call_elapsed_us"]),
              "cpu_input_bytes": (0, (1 << 64) - 1), "cpu_output_bytes": (0, (1 << 64) - 1),
              "cpu_max_tensor_bytes": (1, 32)}
    require(isinstance(profiled, dict) and set(profiled) == set(expected) | set(bounds)
            and all(type(profiled[key]) is int and profiled[key] == size for key, size in expected.items())
            and all(type(profiled[key]) is int and lower <= profiled[key] <= upper
                    for key, (lower, upper) in bounds.items()),
            "bounded FP16 profile timing, complete node coverage or CPU tensor bounds differ")


def validate_layer0_evidence(value, component, roster=LAYER0_TAPS, *, tokens=32):
    taps = value["layer0_taps"]
    require(isinstance(taps, list) and len(taps) == len(roster), "exact diagnostic tap roster required")
    for tap, (name, width) in zip(taps, roster):
        require(isinstance(tap, dict) and set(tap) == {"name", "width", "bits"}
                and tap["name"] == name and type(tap["width"]) is int and tap["width"] == width,
                "layer-0 tap name, order or width differs")
        bits = tap["bits"]
        require(isinstance(bits, list) and len(bits) == tokens * width
                and all(type(word) is int and 0 <= word <= 0xFFFF and word & 0x7C00 != 0x7C00 for word in bits),
                "complete finite raw binary16 layer-0 tap required")

    graph = value["layer0_final_graph"]
    component["validate_graph_metadata"](graph, value["final_placement"])
    rows = graph["rows"]
    values = {row["value"]: row for row in rows if row["kind"] == "value" and row["graph"] == 0}
    outputs = [values[row["value"]] for row in rows
               if row["kind"] == "edge" and row["graph"] == 0 and row["role"] == 4]
    by_name = {row["name"]: row for row in outputs}
    require(len(outputs) == 1 + len(roster) and set(by_name) == {"scores", *(name for name, _ in roster)},
            "exact score and layer-0 graph output boundary required")
    cuda_nodes = {row["node"] for row in rows if row["kind"] == "node" and row["graph"] == 0 and row["ep"] == CUDA}
    widths = {row["value"]: row["number"] for row in rows
              if row["kind"] == "dimension" and row["graph"] == 0 and row["index"] == 2}
    for name, width in roster:
        tap = by_name[name]
        require(tap["type_state"] == "tensor" and tap["element_type"] == 10 and tap["rank"] == 3
                and tap["provenance"] == "node_output" and tap["producer"] in cuda_nodes
                and widths.get(tap["value"]) == width,
                "layer-0 graph output type, width or CUDA origin differs")


def validate_t32_row(rows, mode):
    require(len(rows) == 1 and rows[0]["name"] == "diagnostic-all-live-32"
            and rows[0]["tokens"] == 32 and rows[0]["input_digest"] == T32_DIGEST,
            "exact native T32 row required")
    row = rows[0]
    require(len(row["raw_scores"]) == 8 and all(finite(score, -math.inf, math.inf) for score in row["raw_scores"])
            and finite(row["elapsed_us"], 0, 120_000_000)
            and finite(row["native_call_elapsed_us"], 0, row["elapsed_us"]),
            "complete finite T32 scores and timing required")
    if mode not in TAP_SCOPES:
        validate_t32_comparisons(row)
    return row


def validate_t32(value, paths, owned, request, evidence, metadata, manifest, image, component, base, *,
                 mode="t32", reviewed=None, reviewed_placement=None, policy_source=None,
                 projection=None, projection_reference=None):
    if mode in TAP_SCOPES:
        schema, policy, roster = TAP_SCOPES[mode]
    else:
        schema = (projection_schema(projection, PROJECTION_T32_SCHEMA) if mode in PROJECTION_SCORE_MODES
                  else {"t32": T32_SCHEMA, "bounded-t32": BOUNDED_SCHEMA}[mode])
        policy, roster = BOUNDED_POLICY, None
    validate_native_identity(value, schema, owned, request, evidence, metadata, manifest, image, base)
    if mode in PROJECTION_SCORE_MODES:
        require(value.get("projection_stage") == core(projection[0])
                and value.get("projection_reference") == core(projection_reference),
                "native Q/K/V projection stage or independent CPU proof differs")
    require(value["native_forward_returned"] is value["forward_synchronized"] is True
            and value["numeric_forward_attempts"] == 1, "native T32 return and synchronization required")
    row = validate_t32_row(value["rows"], mode)
    refusal = value["input_refusals"]
    require([item["case"] for item in refusal] == ["wrong_native_positions", "mask_hole", "mask_all_zero"]
            and all(item["refused"] is True and item["native_forward_dispatched"] is False for item in refusal),
            "three native pre-dispatch geometry refusals required")
    plan = value["attention_plan"]
    expected_math = {"gemm": "2", "debug": "1", "unicode": True}
    require(plan["schema"] == "airk.native.cuda-fp16-attention-plan/1"
            and plan["dtype"] == "fp16" and plan["backend"] == "FLASH_ATTENTION"
            and plan["math_environment"] == {"captured_before_ort": expected_math, "current": expected_math}
            and len(plan["nodes"]) == 28 and {node["name"] for node in plan["nodes"]} == NAMES
            and all(node["operator"] == "GroupQueryAttention" and node["domain"] == "com.microsoft"
                    for node in plan["nodes"]), "exact G5 math and planned attention roster required")
    placement = value["final_placement"]
    placed = placement["nodes"]
    require(placement["scope"] == "onnxruntime-final-session-state-ep-placement"
            and placement["complete"] is True and placement["shim_status"] == placement["ort_error_code"] == 0
            and 0 < len(placed) <= 65536 and placement["reported_nodes"] == len(placed)
            and sum(len(node[key].encode("utf-8")) for node in placed for key in ("name", "domain", "op", "ep"))
                == placement["reported_string_bytes"], "complete final placement required")
    if mode in REVIEWED_MODES:
        total = (reviewed_placement["reported_nodes"] if mode == "qkvo-t32" else
                 PROJECTION_TOTAL_NODES if mode == "projection-t32" else BOUNDED_TOTAL_NODES)
        validate_bounded_t32(value, reviewed, reviewed_placement, policy_source, policy=policy, total=total)
        if roster is not None:
            validate_layer0_evidence(value, component, roster)
    else:
        require(all(node["ep"] == CUDA for node in placed), "complete strict CUDA placement required")
    attention = [node for node in placed if node["op"] == "GroupQueryAttention"]
    require(len(attention) == 28 and {node["name"] for node in attention} == NAMES
            and all(node["domain"] == "com.microsoft" and node["subgraph_index"] == 0 and node["ep"] == CUDA
                    for node in attention),
            "complete native CUDA GQA placement required")
    events = read_operations(paths["operations"])
    component["validate_operations"](events, value)
    require(events[-1]["native_call_elapsed_us"] == row["native_call_elapsed_us"],
            "native numeric duration and independent returned marker differ")
    raw = paths["stdout"].read_bytes()
    backend = attention_logs(raw, mode)
    trace = value["runtime_profile"]
    profile_path = Path(trace["path"])
    require(profile_path.parent == paths["stdout"].parent
            and profile_path.name.startswith("ort-profile") and profile_path.suffix == ".json"
            and paths["profile"] == profile_path and 0 < trace["bytes"] <= LOG_CAP,
            "bounded runtime profile escaped its checked output prefix")
    base.pinned(trace)
    return backend


def read_sealed_t1024(request, stage, reviewed, reference, parent, base, audit):
    directory = CAMPAIGN / SEALED_T1024_BATCH
    outer_path, native_path = directory / "execution.json", directory / "native-result.json"
    audit([outer_path, native_path])
    outer_pin = exact_pin(outer_path, SEALED_T1024_EXECUTION_SHA, base)
    native_pin = exact_pin(native_path, SEALED_T1024_NATIVE_SHA, base)
    outer, native = base.read(outer_path), base.read(native_path)
    require(outer["schema"] == "airk.private.owned-compact-fp16-probe/1"
            and outer["mode"] == DEVELOPMENT_MODE and outer["complete"] is True
            and outer["qualification"] is False and outer["failure"] is None
            and parent.settled(outer["owned"]) and outer["outputs"]["native"] == native_pin
            and outer["request"] == request and outer["projection_stage"] == core(stage)
            and outer["projection_reference"] == core(reference) and outer["reviewed_graph"]["native"] == reviewed
            and native["schema"] == DEVELOPMENT_SCHEMA and native["complete"] is True
            and native["development_only"] is native["forward_synchronized"] is True
            and [item["tokens"] for item in native["rows"]] == list(DEVELOPMENT_WIDTHS),
            "exact settled T1024 baseline scoring receipt required")
    sealed = native["rows"][1]
    require(sealed["forward_synchronized"] is True
            and len(sealed["raw_scores"]) == 8
            and all(finite(score, -math.inf, math.inf) for score in sealed["raw_scores"]),
            "finite synchronized sealed T1024 scores required")
    return (outer_pin, native_pin), sealed


def validate_wider_capture(value, paths, owned, request, evidence, metadata, manifest, image, component, base,
                           projection, reference_pin, graph, placement, policy_source, sealed, mode=WIDER_TAP_MODE):
    decision = mode == DECISION_MODE
    schema = "airk.private.compact-fp32-qkv-decision-t1024/1" if decision else WIDER_TAP_SCHEMA
    policy = b"airk.private.compact-fp32-qkv-cuda-decision-t1024/1" if decision else WIDER_TAP_POLICY
    validate_native_identity(value, schema, owned, request, evidence, metadata, manifest, image, base)
    require(value["projection_stage"] == core(projection[0])
            and value["projection_reference"] == core(reference_pin)
            and value["native_forward_returned"] is value["forward_synchronized"] is True
            and value["numeric_forward_attempts"] == 1 and len(value["rows"]) == 1,
            "one synchronized captured T1024 forward required")
    row = value["rows"][0]
    require(row["tokens"] == WIDER_TAP_TOKENS and row["name"] == sealed["name"]
            and row["input_digest"] == sealed["input_digest"]
            and len(row["raw_scores"]) == 8
            and all(finite(score, -math.inf, math.inf) for score in row["raw_scores"])
            and struct.pack("<8f", *row["raw_scores"]) == struct.pack("<8f", *sealed["raw_scores"])
            and finite(row["elapsed_us"], 0, 120_000_000)
            and finite(row["native_call_elapsed_us"], 0, row["elapsed_us"]),
            "captured T1024 input, score words or timing differs from the sealed evidence")
    validate_bounded_t32(value, graph, placement, policy_source, policy=policy,
                         total=PROJECTION_TOTAL_NODES, tokens=WIDER_TAP_TOKENS)
    validate_layer0_evidence(value, component, DECISION_ROSTER if decision else LAYER0_TAPS,
                             tokens=1 if decision else WIDER_TAP_TOKENS)
    events = read_operations(paths["operations"])
    component["validate_operations"](events, value)
    require(events[-1]["native_call_elapsed_us"] == row["native_call_elapsed_us"],
            "captured T1024 operation timing differs")
    plan = value["attention_plan"]
    math_env = {"gemm": "2", "debug": "1", "unicode": True}
    require(plan["schema"] == "airk.native.cuda-fp16-attention-plan/1"
            and plan["dtype"] == "fp16" and plan["backend"] == "FLASH_ATTENTION"
            and plan["math_environment"] == {"captured_before_ort": math_env, "current": math_env}
            and len(plan["nodes"]) == 28 and {item["name"] for item in plan["nodes"]} == NAMES
            and all(item["operator"] == "GroupQueryAttention" and item["domain"] == "com.microsoft"
                    for item in plan["nodes"]), "exact captured FP16 FlashAttention plan required")
    trace = value["runtime_profile"]
    require(Path(trace["path"]) == paths["profile"] and 0 < trace["bytes"] <= LOG_CAP,
            "exact captured T1024 runtime profile required")
    base.pinned(trace)
    return attention_logs(paths["stdout"].read_bytes(), mode)


def development_metrics(raw, reference):
    require(len(raw) == len(reference) and 2 <= len(raw) <= 8, "development reference cardinality differs")
    word_format = f"<{len(raw)}f"

    def f32(values):
        return struct.unpack(word_format, struct.pack(word_format, *values))

    def mass(values):
        peak = max(values)
        weights = [math.exp(value - peak) for value in values]
        total = sum(weights)
        return [value / total for value in weights]

    actual, expected = f32(raw), f32(reference)
    order = sorted(range(len(raw)), key=expected.__getitem__, reverse=True)
    actual_order = sorted(range(len(raw)), key=actual.__getitem__, reverse=True)
    winner, ref_winner = actual_order[0], order[0]
    return {
        "raw_error": max(abs(a - b) for a, b in zip(actual, expected, strict=True)),
        "mass_error": max(abs(a - b) for a, b in zip(mass(actual), mass(expected), strict=True)),
        "argmax_matches": winner == ref_winner,
        "winner": ref_winner,
        "reference_margin": expected[ref_winner] - expected[order[1]],
        "native_margin_for_reference_winner": actual[ref_winner] - max(
            score for index, score in enumerate(actual) if index != ref_winner),
        "native_winner": winner,
        "native_winner_margin": actual[winner] - actual[actual_order[1]],
    }


def provisional_development_pass(matching, original):
    return (matching["raw_error"] <= DEVELOPMENT_RAW_LIMIT and matching["argmax_matches"]
            and matching["mass_error"] <= .05 and original["mass_error"] <= .05)


def development_attention_logs(raw, rows, mode=DEVELOPMENT_MODE):
    require(0 < len(raw) <= LOG_CAP, "complete bounded development log required")
    prefix = f"test {SELECTORS[mode]} ... "
    marker = "AIRK_CUDA_QKV_DEVELOPMENT_FORWARD "
    chunks = []
    for line in raw.decode("utf-8", errors="replace").splitlines():
        line = line.removeprefix(prefix)
        if line.startswith(marker):
            require(len(chunks) < len(rows), "unexpected development forward marker")
            row = rows[len(chunks)]
            require(line == f"{marker}{row['tokens']} {row['input_digest']}",
                    "CUDA development forward marker differs")
            chunks.append([])
        elif "SdpaKernel=" in line:
            require(chunks, "received an attention log before the first development forward")
            chunks[-1].append(line)
    require(len(chunks) == len(rows), "exact development forward marker roster required")
    return [attention_logs("\n".join(chunk).encode("utf-8"), mode) for chunk in chunks]


def validate_development_operations(events, value, component, timing_mode=None):
    rows = value["rows"]
    require(len(events) == 2 * (1 + len(rows)), "exact development Load and Numeric journals required")
    component["validate_operations"](events[:2], value, load_only=True)
    identity = events[0]["identity"]
    previous = events[1]["observed_monotonic_ns"]
    for ordinal, row in enumerate(rows, 1):
        start, end = events[2 * ordinal:2 * ordinal + 2]
        if timing_mode == "qkv-resource20":
            kind, native_ordinal = ("warmup" if ordinal == 1 else "sample"), ordinal - 1
        elif timing_mode in ("qkv-mixed", "qkv-fit-probe"):
            kind, native_ordinal = "sample", ordinal
        else:
            kind, native_ordinal = "numeric", ordinal
        for turn, phase in ((start, "started"), (end, "returned")):
            sequence = 2 * ordinal + (1 if phase == "started" else 2)
            require(turn["schema"] == "airk.arbiter.native-operation/1"
                    and turn["sequence"] == sequence and turn["identity"] == identity
                    and turn["kind"] == kind and turn["ordinal"] == native_ordinal
                    and turn["phase"] == phase and turn["clock"] == "windows_performance_counter"
                    and turn["operation_input_digest"] == row["input_digest"]
                    and turn["expires_monotonic_ns"] == value["original_expires_monotonic_ns"]
                    and previous <= turn["observed_monotonic_ns"] < turn["expires_monotonic_ns"],
                    "development operation identity, order or deadline differs")
            previous = turn["observed_monotonic_ns"]
        require(start["started_monotonic_ns"] == start["observed_monotonic_ns"] == end["started_monotonic_ns"]
                and start["native_success"] is start["native_call_elapsed_us"] is None
                and end["native_success"] is True
                and end["native_call_elapsed_us"] == row["native_call_elapsed_us"],
                "development Numeric operation timing or completion differs")


FULL_CUT_GROUPS = {
    "head": {
        "/scorer/layers.0/input_layernorm/Mul_1_output_0": 2048,
        "/scorer/layers.13/input_layernorm/Mul_1_output_0": 2048,
        "/scorer/layers.27/input_layernorm/Mul_1_output_0": 2048,
        "/scorer/norm/Mul_1_output_0": 2048,
    },
    "attention": {
        "/airk_g5/attention/13/q16": 2048,
        "/airk_g5/attention/13/k16": 1024,
        "/airk_g5/attention/13/v16": 1024,
        "/airk_g5/attention/13/raw16": 2048,
    },
    "dense": {
        "/airk_g5/attention/13/raw16": 2048,
        "/scorer/layers.13/self_attn/o_proj/MatMul_output_0": 2048,
        "/scorer/layers.13/mlp/Mul_output_0": 6144,
        "/scorer/layers.13/mlp/down_proj/MatMul_output_0": 2048,
    },
}


def validate_full_cut(value, base, group):
    sealed = exact_pin(CAMPAIGN / "compact-fp16-full-engineering-score-r1/native-result.json",
                       "735c77839099adf75426cf3ef9884ad69a99b60bb73e5fdf6247b7f321b12ded", base)
    prior = base.read(sealed["path"])
    require(value["rows"][0]["raw_scores"] == prior["rows"][0]["raw_scores"]
            and value["rows"][0]["compiled"] == prior["rows"][0]["compiled"],
            "full-projection diagnostic perturbed the sealed score or input")
    require(group in FULL_CUT_GROUPS and value["full_cut_group"] == group, "full diagnostic group identity differs")
    names = FULL_CUT_GROUPS[group]
    rows = value["full_cut_rows"]
    require(len(rows) == len(names) and {row["name"] for row in rows} == set(names), "full cut roster differs")
    for row in rows:
        name = row["name"]
        tokens = 1014 if name in ("/airk_g5/attention/13/k16", "/airk_g5/attention/13/v16") else 1
        require(row["width"] == names[name] and row["tokens_retained"] == tokens
                and len(row["words"]) == tokens * names[name] * (2 if name.startswith("/scorer/") else 1)
                and all(type(word) is int and 0 <= word <= 65535 for word in row["words"]), "invalid cut payload")


def validate_development_placement(value, placement, graph, source, *, policy=BOUNDED_POLICY):
    actual_placement = value["final_placement"]
    require(actual_placement["complete"] is True
            and actual_placement["shim_status"] == actual_placement["ort_error_code"] == 0
            and placement_roster(actual_placement) == placement_roster(placement)
            and value["reviewed_graph"] == core(graph)
            and value["bounded_policy_identity"] == bounded_policy_identity(
                graph["sha256"], value["profile"], source["sha256"], domain=policy),
            "development placement or reviewed graph policy differs")
    plan = value["attention_plan"]
    expected_math = {"gemm": "2", "debug": "1", "unicode": True}
    require(plan["schema"] == "airk.native.cuda-fp16-attention-plan/1"
            and plan["dtype"] == "fp16" and plan["backend"] == "FLASH_ATTENTION"
            and plan["math_environment"] == {"captured_before_ort": expected_math, "current": expected_math}
            and len(plan["nodes"]) == 28 and {node["name"] for node in plan["nodes"]} == NAMES
            and all(node["operator"] == "GroupQueryAttention" and node["domain"] == "com.microsoft"
                    for node in plan["nodes"]), "exact development FP16 FlashAttention plan required")


def validate_development(value, paths, owned, request, evidence, metadata, manifest, image, component, base,
                         projection, reference_pin, reference_proof, graph, placement, source):
    validate_native_identity(value, DEVELOPMENT_SCHEMA, owned, request, evidence, metadata, manifest, image, base)
    require(value["development_only"] is True and value["qualification_cutoff_raw"] == RELEASE_RAW_LIMIT
            and value["historical_qualification_cutoff_raw"] == .001
            and value["development_raw_limit"] == DEVELOPMENT_RAW_LIMIT
            and value["matching_qkv_mass_limit"] == value["original_fp32_mass_limit"] == .05
            and value["near_tie_margin_limit"] == DEVELOPMENT_NEAR_TIE
            and value["reference_basis"] == "independent_wider_qkv_cpu_approximation"
            and value["reference_recipe"] == reference_proof["recipe"]
            and value["projection_stage"] == core(projection[0]) and value["projection_reference"] == core(reference_pin)
            and value["native_forward_returned"] is value["forward_synchronized"] is True
            and value["numeric_forward_attempts"] == len(DEVELOPMENT_WIDTHS),
            "development scope, cutoffs or comparison identity differs")
    validate_development_placement(value, placement, graph, source)
    rows, profiles = value["rows"], value["profiled_runs"]
    require(len(rows) == len(profiles) == len(DEVELOPMENT_WIDTHS), "exact development result and profile roster required")
    adapter_identity = ("uuid", "ordinal", "compute_major", "compute_minor", "driver_api_version",
                        "driver_image_sha256", "physical_total_bytes")
    gate_results = []
    for row, profile, reference, width in zip(rows, profiles, reference_proof["references"], DEVELOPMENT_WIDTHS, strict=True):
        require(row["tokens"] == width and row["name"] == reference["name"]
                and row["input_digest"] == reference["input"]["input_digest"]
                and row["forward_synchronized"] is True
                and len(row["raw_scores"]) == 8 and all(finite(score, -math.inf, math.inf) for score in row["raw_scores"])
                and finite(row["elapsed_us"], 0, 120_000_000)
                and finite(row["native_call_elapsed_us"], 0, row["elapsed_us"]),
                "complete finite synchronized development row required")
        for when in ("before", "after"):
            adapter = row[f"adapter_{when}"]
            require(all(adapter[key] == value["adapter_before"][key] for key in adapter_identity)
                    and 0 <= adapter["free_bytes"] <= adapter["total_bytes"] <= adapter["physical_total_bytes"]
                    and adapter["total_bytes"] > 0 and finite(row[f"host_rss_{when}_bytes"], 1, HOST),
                    "development memory observation or adapter identity differs")
        matching = development_metrics(row["raw_scores"], reference["matching_qkv_fp32"])
        original = development_metrics(row["raw_scores"], reference["original_fp32"])
        for label, stats in (("matching_qkv", matching), ("original_fp32", original)):
            require(row[f"{label}_raw_error"] == stats["raw_error"]
                    and math.isclose(row[f"{label}_mass_error"], stats["mass_error"], rel_tol=0.0, abs_tol=2e-15)
                    and stats["mass_error"] <= .05
                    and row[f"{label}_argmax_matches"] is stats["argmax_matches"]
                    and row[f"{label}_winner"] == stats["winner"]
                    and row[f"{label}_reference_margin"] == stats["reference_margin"]
                    and row[f"native_margin_for_{label}_winner"] == stats["native_margin_for_reference_winner"],
                    "development raw, mass or winner comparison could not be reproduced")
        require(row["native_winner"] == matching["native_winner"]
                and row["native_winner_margin"] == matching["native_winner_margin"]
                and row["within_deadline"] is True,
                "native winner, margin or original deadline differs")
        near_tie = matching["reference_margin"] <= DEVELOPMENT_NEAR_TIE
        screened = provisional_development_pass(matching, original)
        require(row["raw_ceiling_winner_at_risk"] is (matching["reference_margin"] <= 2 * DEVELOPMENT_RAW_LIMIT),
                "development winner at-risk label differs")
        require(row["near_tie_ambiguous"] is near_tie and row["development_screen_pass"] is screened,
                "development near-tie or raw-screen classification differs")
        gate_results.append(screened)
        require(profile["tokens"] == width and profile["node_count"] == PROJECTION_TOTAL_NODES
                and profile["cpu_nodes"] == BOUNDED_CPU_NODES
                and finite(profile["model_run_us"], 1, row["native_call_elapsed_us"])
                and finite(profile["cpu_input_bytes"], 0, (1 << 64) - 1)
                and finite(profile["cpu_output_bytes"], 0, (1 << 64) - 1)
                and finite(profile["cpu_max_tensor_bytes"], 1, 32),
                "development profile does not cover the exact bounded graph")
    require(value["development_screen_pass"] is all(gate_results), "development batch screen differs")
    validate_development_operations(read_operations(paths["operations"]), value, component)
    trace = value["runtime_profile"]
    profile_path = Path(trace["path"])
    require(profile_path.parent == paths["stdout"].parent and profile_path.name.startswith("ort-profile")
            and profile_path.suffix == ".json" and paths["profile"] == profile_path and 0 < trace["bytes"] <= LOG_CAP,
            "development profile escaped its checked output prefix")
    base.pinned(trace)
    return development_attention_logs(paths["stdout"].read_bytes(), rows)


def run(args, *, capacity_gate=None):
    require(capacity_gate is None or (callable(capacity_gate) and args.mode in ("qkv-fit-probe", "qkv-resource20")),
            "external capacity gate requires an owned fit or fixed-population probe")
    started = time.monotonic()
    expires = started + WALL
    started_utc = datetime.now(timezone.utc)
    require(os.name == "nt" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "use isolated Windows Python -I -B -S")
    component = runpy.run_path(str(ROOT / "scripts/cuda-component-execution.py"))
    base = component["base"]()
    parent, owner = base.helpers({"ownership_helper_root": str(base.MAINTAINED_HELPER_ROOT)})
    require(parent.worker.windows_sid() == base.SID and component["token_owner"](ctypes.c_void_p(-1)) == base.SID,
            "actual parent token differs")
    guard = {"checked_guard": base.pin(ROOT / "scripts/dev/assert-human-execution.ps1"),
             "powershell": exact_pin(args.powershell, args.powershell_sha256, base)}

    def audit(paths, created=True, *, deadline=expires):
        component["guard_many"](guard, paths, created, deadline=deadline)

    out = args.output
    require(out.is_absolute() and out.parent.resolve(strict=True) == out.parent
            and out.parent == CAMPAIGN and not out.exists(), "fresh campaign child required")
    manifest_path = args.image.with_name("airk-cuda-runtime.json")
    audit([args.request, args.evidence, args.image, args.build_receipt, manifest_path])
    request = exact_pin(args.request, args.request_sha256, base)
    evidence = exact_pin(args.evidence, args.evidence_sha256, base)
    image = exact_pin(args.image, args.image_sha256, base)
    build = exact_pin(args.build_receipt, args.build_receipt_sha256, base)
    manifest = base.pin(manifest_path)
    request_value, evidence_value = base.read(request["path"]), base.read(evidence["path"])
    metadata_mode = args.mode in METADATA_MODES
    graph_mode = args.mode in GRAPH_MODES
    projection = (projection_stage(args.projection_stage, args.projection_stage_sha256, base, audit,
                                   output_projection=args.mode in QKVO_MODES, down=args.mode in DOWN_MODES, full=args.mode in FULL_MODES)
                  if args.mode in PROJECTION_MODES else None)
    validate_request(args.mode, request_value, request, evidence_value, manifest, projection=projection)
    retained = cargo_chain = None
    if getattr(args, "retained_cargo_proof", None) is not None:
        retained, cargo_chain = retained_cargo_chain(args.retained_cargo_proof, args.retained_cargo_proof_sha256,
                                                    build, image, manifest, base, audit, parent)
    log, artifact, sources = linked_image(build, image, base, audit, args.mode, cargo_chain=cargo_chain,
                                          source_snapshot=getattr(args, "source_snapshot", None))
    dependencies = {}
    if metadata_mode:
        if projection is not None:
            validate_projection_baseline(projection[1], evidence_value)
        else:
            dependencies = evidence_value["candidate"]
            require(set(dependencies) == {"graph", "weights"}, "complete candidate graph and sidecar pins required")
            audit([Path(pin["path"]) for pin in dependencies.values()])
            for pin in dependencies.values():
                base.pinned(pin)
    else:
        for name in ("profile_pin", "native_input", "source_manifest", "independent_proof"):
            pin = evidence_value[name]
            audit([Path(pin["path"])])
            dependencies[name] = retained_core(pin, base)
    inputs = [request, evidence, image, build, manifest, log, artifact, *sources.values(), *dependencies.values()]
    if retained is not None:
        inputs.append(retained)
    approvals = decision_build_approvals(log, artifact, manifest, base, audit, cargo_chain=cargo_chain) if args.mode in (CAL_MODE, DECISION_MODE, OBSERVED_MODE, *TIMING_MODES, *DOWN_MODES, *FULL_MODES) else []
    inputs.extend(approvals)
    projection_reference = development_reference = None
    if projection is not None:
        stage_pin, stage = projection
        inputs.extend((stage_pin, stage["candidate_graph"], stage["candidate_weights"]))
        if not metadata_mode:
            profile = base.read(dependencies["profile_pin"]["path"])
            validate_projection_artifacts(dependencies, profile, projection, base)
            independent = base.read(dependencies["independent_proof"]["path"])
            validate_projection_baseline(stage, independent)
            if args.mode in WIDER_REFERENCE_MODES:
                projection_reference, development_reference = read_development_reference(
                    args.projection_reference, args.projection_reference_sha256,
                    projection, independent, base, audit, expires,
                )
                inputs.append(projection_reference)
                for row in development_reference["references"]:
                    for pin in (row["fixture"], row["producer"]):
                        if pin not in inputs:
                            audit([Path(pin["path"])])
                            base.pinned(pin)
                            inputs.append(pin)
            elif args.mode in (DOWN_SCORE, *FULL_OBSERVED):
                full = args.mode in FULL_OBSERVED
                reference_sha = FULL_REFERENCE_SHA if full else DOWN_REFERENCE_SHA
                reference_schema = ("airk.private.compact-full-projection-reference/1" if full
                                    else "airk.private.compact-mlp-down-projection-reference/1")
                recipe = ("c073724d2f98ac90a944718a055d8326691e569c29731243cf305eb41fb9bd5f" if full
                          else "762f3da4ab62a3b52a528e7aa238c3749aed4af5db7318d777746947eaec58b0")
                audit([args.projection_reference])
                projection_reference = exact_pin(args.projection_reference, reference_sha, base)
                down_closure = base.read(projection_reference["path"])
                down_reference = down_closure["result"]
                require(parent.settled(down_closure["owned"]) and down_closure["complete"] is True
                        and down_reference["schema"] == reference_schema
                        and down_reference["qualification"] is False and down_reference["native_candidate_scoring"] is False
                        and down_reference["recipe_sha256"] == recipe,
                        "protected reference original CPU custody or recipe differs")
                inputs.append(projection_reference)
            elif args.mode in PROJECTION_SCORE_MODES and args.mode != CAL_MODE:
                projection_reference = read_projection_reference(
                    args.projection_reference, args.projection_reference_sha256, projection, independent, base, audit,
                )
                inputs.append(projection_reference)
    graph_receipt = graph = graph_operations = reviewed_placement = None
    if args.mode in REVIEWED_MODES:
        audit([args.graph_receipt])
        expected = (FULL_GRAPH_RECEIPT_SHA if args.mode in FULL_OBSERVED else DOWN_GRAPH_RECEIPT_SHA if args.mode == DOWN_SCORE else QKVO_REVIEWED_RECEIPT_SHA if has_output_projection(projection) else
                    PROJECTION_REVIEWED_RECEIPT_SHA if projection is not None else REVIEWED_RECEIPT_SHA)
        graph_receipt = exact_pin(args.graph_receipt, expected, base)
        graph, graph_operations, reviewed_placement = reviewed_graph(
            graph_receipt, request, evidence, evidence_value, manifest, parent, component, base, audit,
            projection=projection,
        )
        inputs.extend((graph_receipt, graph, graph_operations))
    representative = args.mode in (OBSERVED_MODE, DOWN_SCORE, *FULL_OBSERVED)
    if representative:
        require(args.observed_reference_sha256 in OBSERVED_REFERENCES, "unreviewed engineering reference")
        audit([args.observed_reference])
        reference_pin = exact_pin(args.observed_reference, args.observed_reference_sha256, base)
        closure = base.read(reference_pin["path"])
        reference = closure["result"]
        require(closure["complete"] is True and parent.settled(closure["owned"])
                and reference["reference_basis"]["sha256"] == DEVELOPMENT_REFERENCE_SHA,
                "original CPU source or Job settlement differs")
        if args.mode in (DOWN_SCORE, *FULL_OBSERVED):
            full = args.mode in FULL_OBSERVED
            prior = down_reference["sealed_qkv"] if full else down_reference["baseline"]["scores"]
            match = down_reference["matching_fp16"] if full else down_reference["all_down_fp32"]["scores"]
            require(down_reference["original_cpu"] == reference_pin and down_reference["compiled"] == reference["compiled"]
                    and down_reference["roster"] == reference["roster"] and prior == reference["matching_fp16"]
                    and down_reference["original_fp32"] == reference["original_fp32"], "protected reference original input differs")
            reference = dict(reference, matching_fp16=match)
        inputs.append(reference_pin)
        for file in (reference["census"], reference["capture"]):
            audit([Path(file["path"])])
            inputs.append(exact_pin(Path(file["path"]), file["sha256"], base))
    sealed_t1024 = None
    timing = args.mode in TIMING_MODES
    scientific = args.mode == CAL_MODE
    cal_pins = {}
    if scientific:
        for name in ("preflight", "original_compiled", "references"):
            path = getattr(args, "cal_" + name)
            audit([path])
            require(path.is_absolute() and path.resolve() == path and path.is_relative_to(CAMPAIGN), "foreign CAL input")
            cal_pins[name] = exact_pin(path, getattr(args, "cal_" + name + "_sha256"), base)
        inputs.extend(cal_pins.values())
    monitor = timing or representative or scientific
    if args.mode in WIDE_CAPTURES or timing:
        sealed_pins, sealed_t1024 = read_sealed_t1024(
            request, stage_pin, graph, projection_reference, parent, base, audit,
        )
        inputs.extend(sealed_pins)
    if monitor:
        scripts = (ROOT / "scripts/arbiter-fp16-execution/timing_control.py",
                   ROOT / "scripts/cuda-component-execution/wddm_process_samples.py",
                   ROOT / "scripts/cuda-component-execution/wddm_capacity_snapshot.py")
        audit(scripts)
        controls = [base.pin(path) for path in scripts]
        inputs.extend(controls)
        timing_owner, wddm, capacity = [runpy.run_path(str(path)) for path in scripts]
    if timing:
        short_folder = CAMPAIGN / "compact-fp16-qkv-fp32-native-t32-r2"
        short_source = short_folder / "native-result.json"
        short_owner = short_folder / "execution.json"
        audit([short_source, short_owner])
        short = exact_pin(short_source, "217431438d1bf79bbffa071c9d69d62f4dcb73c44cdee458c19cc50a544d774f", base)
        short_receipt = exact_pin(short_owner, "a479f953b5c13076d7c54388a89e257f6a18dc3cc5031a3bae9fa89e2b99e3ee", base)
        previous = base.read(str(short_owner))
        require(previous["request"] == request and previous["projection_stage"] == core(stage_pin)
                and previous["complete"] is False and previous["owned"]["owned_descendants_settled"] is True
                and short in previous["failure_outputs"], "original T32 refusal or candidate identity differs")
        inputs.extend([short, short_receipt])
        sealed_rows = {row["tokens"]: row for row in (base.read(str(short_source))["rows"]
                       + base.read(sealed_pins[1]["path"])["rows"])}
    command = [image["path"], "--ignored", "--exact", SELECTORS[args.mode], "--nocapture", "--test-threads=1"]
    receipt = {
        "schema": "airk.private.owned-compact-fp16-probe/1", "mode": args.mode, "complete": False,
        "qualification": False, "failure": None, "started_utc": started_utc.isoformat(),
        "original_deadline_utc": (started_utc + timedelta(seconds=WALL)).isoformat(),
        "original_deadline_monotonic": expires, "wall_seconds": WALL, "command": command,
        "request": request, "reviewed_evidence": evidence, "image": image, "build": build,
        "build_log": log, "linked_image": artifact, "sources": sources, "runtime_manifest": manifest,
        "execution_user_sid": base.SID, "outputs": {},
    }
    if graph is not None:
        receipt["reviewed_graph"] = {"owned": graph_receipt, "native": graph, "operations": graph_operations}
    if projection is not None:
        receipt["projection_stage"] = core(stage_pin)
        receipt["projection_assets"] = {name: stage[name] for name in ("candidate_graph", "candidate_weights")}
    if projection_reference is not None:
        receipt["projection_reference"] = core(projection_reference)
    if retained is not None:
        receipt["retained_cargo_proof"] = retained
        receipt["original_compiler_image"] = cargo_chain["linked_image"]
    if approvals:
        receipt["compile_time_cuda_approvals"] = approvals
    if timing:
        receipt["timing_inputs"] = [short_receipt, short, *sealed_pins, *controls]
    if sealed_t1024 is not None:
        receipt["sealed_scoring"] = {"owned": sealed_pins[0], "native": sealed_pins[1]}
    component["mkdir"](guard, out, deadline=expires)
    temp = out / "temp"
    metadata = out / "metadata"
    common = {"stdin": out / "stdin.txt", "stdout": out / "stdout-stderr.log"}
    paths = dict(common, metadata=metadata / "metadata-receipt.json") if metadata_mode else dict(
        common, native=out / "native-result.json", operations=out / "native-result.operations.jsonl",
    )
    receipt_path = out / "execution.json"
    try:
        component["mkdir"](guard, temp, deadline=expires)
        planned = [receipt_path, *common.values()]
        if metadata_mode:
            planned.append(metadata)
        else:
            planned.extend((paths["native"], paths["operations"]))
            if args.mode in SCORING_MODES and not timing and not representative and not scientific:
                planned.append(out / "ort-profile")
        audit(planned, False)
        env = {key: value for key, value in os.environ.items()
               if not key.upper().startswith(("AIRK_", "ORT_", "PYTHON", "CUDA_", "CUDNN_"))}
        env.update(PATH=r"C:\Windows\System32;C:\Windows", TEMP=str(temp), TMP=str(temp), TMPDIR=str(temp),
                   AIRK_PRIVATE_CENSUS_BASE=str(temp), CUDA_CACHE_PATH=str(temp),
                   AIRK_CUDA_QUALIFICATION_INPUT=request["path"], AIRK_CUDA_QUALIFICATION_INPUT_SHA256=request["sha256"])
        if scientific:
            env["AIRK_CUDA_CAL_ORDINAL"] = str(args.cal_ordinal)
            for name, pin in cal_pins.items():
                key = "AIRK_CUDA_CAL_" + name.upper()
                env[key], env[key + "_SHA256"] = pin["path"], pin["sha256"]
            receipt["cal_inputs"] = cal_pins
        if args.mode == FULL_CUT:
            require(args.full_cut_group in FULL_CUT_GROUPS, "one declared four-output group required")
            env["AIRK_CUDA_COMPACT_FULL_CUT_GROUP"] = args.full_cut_group
            receipt["full_cut_group"] = args.full_cut_group
        if representative:
            env.update(AIRK_CUDA_OBSERVED_REFERENCE=reference_pin["path"],
                       AIRK_CUDA_OBSERVED_REFERENCE_SHA256=reference_pin["sha256"])
            receipt["observed_reference"] = reference_pin
        if projection is not None:
            env.update(AIRK_CUDA_FP16_PROJECTION_STAGE=stage_pin["path"],
                       AIRK_CUDA_FP16_PROJECTION_STAGE_SHA256=stage_pin["sha256"])
        if projection_reference is not None:
            env.update(AIRK_CUDA_FP16_PROJECTION_REFERENCE=projection_reference["path"],
                       AIRK_CUDA_FP16_PROJECTION_REFERENCE_SHA256=projection_reference["sha256"])
        if metadata_mode:
            env.update(AIRK_CUDA_FP16_INDEPENDENT_PROOF=evidence["path"],
                       AIRK_CUDA_FP16_INDEPENDENT_PROOF_SHA256=evidence["sha256"],
                       AIRK_CUDA_METADATA_DIRECTORY=str(metadata))
        else:
            env.update(AIRK_CUDA_FP16_METADATA_RECEIPT=evidence["path"],
                       AIRK_CUDA_FP16_METADATA_RECEIPT_SHA256=evidence["sha256"],
                       AIRK_CUDA_QUALIFICATION_OUTPUT=str(paths["native"]),
                       ORT_CUDA_GEMM_OPTIONS="2", ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO="1")
            if args.mode in SCORING_MODES and not timing and not representative and not scientific:
                env["AIRK_CUDA_FP16_PROFILE_PREFIX"] = str(out / "ort-profile")
            if graph is not None:
                env.update(AIRK_CUDA_FP16_REVIEWED_GRAPH=graph["path"],
                           AIRK_CUDA_FP16_REVIEWED_GRAPH_SHA256=graph["sha256"])
            receipt["inherited_math_environment"] = {key: env[key] for key in
                ("ORT_CUDA_GEMM_OPTIONS", "ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO")}

        if monitor:
            receipt["before_load_adapter"] = capacity["snapshot"](None)
            luid = receipt["before_load_adapter"]["dxgi"]["luid"]
        if timing:
            env.update(AIRK_CUDA_FP16_TIMING_T32=short["path"], AIRK_CUDA_FP16_TIMING_T32_SHA256=short["sha256"],
                       AIRK_CUDA_FP16_TIMING_WIDER=sealed_pins[1]["path"],
                       AIRK_CUDA_FP16_TIMING_WIDER_SHA256=sealed_pins[1]["sha256"])
        baseline = None

        def before_resume(api, job, info, child_deadline):
            nonlocal baseline
            proof = parent.suspended_authentication(api, info, expires, child_deadline, base.SID,
                                                    image, owner.image_proof)
            require(component["token_owner"](info.process) == base.SID
                    and owner.job_snapshot(api, job) == ([int(info.pid)], 1, 1),
                    "suspended Compact default owner or Job census differs")
            if monitor:
                with wddm["ProcessSampler"](info, luid) as sampler:
                    baseline = sampler.sample()
            limits = job_limits(api, job)
            if capacity_gate is not None:
                measured = capacity_gate(int(info.pid), child_deadline)
                require(isinstance(measured, dict) and measured["candidate_pid"] == info.pid
                        and measured["child_deadline"] == child_deadline and time.monotonic() < child_deadline,
                        "before-resume capacity evidence differs from the exact candidate or deadline")
                receipt["capacity_before_resume"] = measured
            return dict(proof, default_owner_sid=base.SID, job_limits=limits)

        def watch_timing(api, _job, info, child_deadline):
            observation = wddm["observe_process"](api, info, child_deadline, luid, baseline)
            return {"wddm": observation}, []

        with paths["stdin"].open("xb"):
            pass
        with paths["stdin"].open("rb") as stdin, paths["stdout"].open("xb") as stdout:
            audit(common.values())
            receipt["owned"] = owner.run_windows_owned_training(
                command, ROOT, stdout, expires - time.monotonic(), env, stdin, HOST,
                before_resume=before_resume, after_resume=watch_timing if monitor else None, absolute_deadline=expires,
            )
        exit_code = observed_native_exit(receipt["owned"])
        if monitor and receipt["owned"]["owned_descendants_settled"]:
            receipt["after_release_adapter"] = capacity["snapshot"](None)
        observed = list(out.rglob("*"))
        audit(observed)
        require(paths["stdout"].stat().st_size <= LOG_CAP, "actual stdout exceeded its retained bound")
        if args.mode in SCORING_MODES and not timing and not representative and not scientific:
            traces = list(out.glob("ort-profile*.json"))
            if len(traces) == 1:
                paths["profile"] = traces[0]
        value = None
        if not metadata_mode and paths["native"].is_file():
            value = (base.read(paths["native"], LOG_CAP) if args.mode in WIDE_CAPTURES
                     else base.read(paths["native"]))
            if value.get("error"):
                raise ValueError(value["error"])
        require(exit_code == 0, f"native child exited with Windows status 0x{exit_code:08X}")
        require(parent.settled(receipt["owned"])
                and finite(receipt["owned"]["peak_host_committed_bytes"], 1, HOST),
                "successful native child Job or peak host proof differs")
        if metadata_mode:
            value = base.read(paths["metadata"])
            receipt["metadata_artifacts"] = validate_metadata(
                value, paths, request, evidence, manifest, base, projection=projection,
            )
        elif graph_mode:
            require(value is not None, "native graph diagnostic result missing")
            schema = projection_schema(projection, PROJECTION_GRAPH_SCHEMA) if projection is not None else GRAPH_SCHEMA
            validate_native_identity(value, schema, receipt["owned"], request, evidence,
                                     evidence_value, manifest, image, base)
            validate_graph(value, read_operations(paths["operations"]), component)
            if projection is not None:
                require(value.get("projection_stage") == core(stage_pin), "Q/K/V projection graph stage differs")
                placement = value["graph_observation"]["placement"]
                require(sum(node["ep"] == CPU for node in placement["nodes"]) == BOUNDED_CPU_NODES,
                        "Compact reviewed CPU node count differs")
                receipt["projection_graph"] = validate_projection_graph(
                    value["graph_observation"]["graph_metadata"], output_projection=has_output_projection(projection),
                    down=args.mode in DOWN_MODES, full=args.mode in FULL_MODES,
                )
        elif scientific:
            validate_native_identity(value, "airk.private.compact-qkv-scientific-cal-row/1", receipt["owned"], request, evidence, evidence_value, manifest, image, base)
            validate_development_placement(value, reviewed_placement, graph, scientific_validator_source(sources))
            events = read_operations(paths["operations"])
            validate_development_operations(events, value, component)
            witness = value["scientific_cal"]
            compiled = base.read(cal_pins["original_compiled"]["path"], 256 << 20)["rows"][args.cal_ordinal]
            actual = value["cal_row"]
            require(all(witness["independent_references" if name == "references" else name] == core(pin) for name, pin in cal_pins.items()) and witness["ordinal"] == args.cal_ordinal
                    and value["native_forward_returned"] is value["forward_synchronized"] is True
                    and value["numeric_forward_attempts"] == len(value["rows"]) == 1
                    and actual["status"] == "raw_scores_only" and compiled["status"] == "compiled_original_input"
                    and all(actual[k] == compiled[k] for k in ("id", "source_group", "split", "roster", "compiled"))
                    and value["rows"][0]["input_digest"] == compiled["compiled"]["input_digest"]
                    and len(actual["scores"]) == len(compiled["roster"]["cards"]) and all(math.isfinite(v) for v in actual["scores"]),
                    "scientific original input, return or complete card scores differ")
            timing_owner["same_adapter"](receipt["before_load_adapter"], receipt["after_release_adapter"], value)
            series = receipt["owned"]["before_GO"]["wddm"]
            require(series["pid"] == value["pid"] and series["adapter_luid"] == luid and
                    series["observed_exit_monotonic_ns"] >= events[-1]["observed_monotonic_ns"], "foreign CAL WDDM observation")
            receipt["flash_attention_records"] = attention_logs(paths["stdout"].read_bytes(), args.mode)
        elif representative:
            schema = ("airk.private.compact-full-t1014-cut/1" if args.mode == FULL_CUT else
                      "airk.private.compact-full-observed-development/1" if args.mode in FULL_OBSERVED
                      else "airk.private.compact-qkv-down-observed-development/1" if args.mode == DOWN_SCORE
                      else "airk.private.compact-qkv-observed-development/1")
            validate_native_identity(value, schema, receipt["owned"],
                                     request, evidence, evidence_value, manifest, image, base)
            require(value["profiling_enabled"] is False and value["runtime_profile"] is None
                    and not any(out.glob("ort-profile*")) and value["projection_stage"] == core(stage_pin)
                    and value["projection_reference"] == core(projection_reference)
                    and value["observed_reference"] == core(reference_pin)
                    and all(value["observed_" + key] == core(reference[key]) for key in ("capture", "census"))
                    and value["native_forward_returned"] is value["forward_synchronized"] is True
                    and value["numeric_forward_attempts"] == len(value["rows"]) == 1,
                    "representative source, original receipt or unprofiled scoring differs")
            row, = value["rows"]
            compiled = reference["compiled"]
            capture = base.read(reference["capture"]["path"])
            require(row["name"] == reference["case"] and row["ordinal"] == reference["ordinal"]
                    and row["compiled"] == compiled and row["input_digest"] == compiled["input_digest"]
                    and row["tokens"] == len(compiled["input_ids"]) <= reference["remaining_input_tokens"]
                    and row["options"] == len(reference["roster"]["cards"]) and row["roster"] == reference["roster"]
                    and row["matching_fp16"] == reference["matching_fp16"] and row["original_fp32"] == reference["original_fp32"]
                    and row["original_remaining_ms"] == capture["request"]["remaining_ms"]
                    and row["original_remaining_ns"] == capture["remaining_ns_at_observation"]
                    and row["within_deadline"] is row["forward_synchronized"] is True,
                    "changed original input, roster, independent scores or remaining native deadline")
            stats = {}
            for label, key in (("matching", "matching_fp16"), ("original", "original_fp32")):
                result = development_metrics(row["raw_scores"], reference[key])
                require(row[label + "_raw_error"] == result["raw_error"] and row[label + "_winner_agrees"] is result["argmax_matches"]
                        and math.isclose(row[label + "_mass_error"], result["mass_error"], rel_tol=0, abs_tol=2e-15),
                        "representative score comparison differs")
                stats[label] = result
            matching = stats["matching"]
            passed = provisional_development_pass(matching, stats["original"])
            require(row["matching_margin"] == matching["reference_margin"] and row["development_screen_pass"] is passed
                    and row["near_tie_ambiguous"] is (matching["reference_margin"] <= DEVELOPMENT_NEAR_TIE)
                    and row["raw_ceiling_winner_at_risk"] is (matching["reference_margin"] <= 2 * DEVELOPMENT_RAW_LIMIT),
                    "development decision or margin differs")
            policy = BOUNDED_POLICY
            if args.mode == FULL_CUT:
                policy = b"airk.private.compact-full-fp32-cuda-t1014-cuts/1"
                validate_full_cut(value, base, args.full_cut_group)
            validate_development_placement(value, reviewed_placement, graph, sources[BOUNDED_SOURCE], policy=policy)
            events = read_operations(paths["operations"])
            validate_development_operations(events, value, component)
            timing_owner["same_adapter"](receipt["before_load_adapter"], receipt["after_release_adapter"], value)
            series = receipt["owned"]["before_GO"]["wddm"]
            require(series["pid"] == value["pid"] and series["adapter_luid"] == luid
                    and series["observed_exit_monotonic_ns"] >= events[-1]["observed_monotonic_ns"],
                    "WDDM samples do not bind the native execution")
            resident = [s for s in series["samples"] if all(type(s["process"].get(k)) is int
                        for k in ("Dedicated Usage", "Shared Usage")) and s["process"]["Dedicated Usage"] > 0]
            call = [s for s in resident if events[2]["observed_monotonic_ns"] <= s["start_monotonic_ns"]
                    <= s["end_monotonic_ns"] <= events[3]["observed_monotonic_ns"]]
            receipt["residency"] = {
                "sample_count": len(resident), "within_numeric_sample_count": len(call),
                "observed_dedicated_max_bytes": max((s["process"]["Dedicated Usage"] for s in resident), default=None),
                "observed_shared_max_bytes": max((s["process"]["Shared Usage"] for s in resident), default=None),
                "within_numeric_dedicated_max_bytes": max((s["process"]["Dedicated Usage"] for s in call), default=None),
            }
            receipt["flash_attention_records"] = attention_logs(paths["stdout"].read_bytes(), args.mode)
            receipt["observed_comparison"] = stats
            receipt["development_raw_limit"] = DEVELOPMENT_RAW_LIMIT
            receipt["development_screen_pass"] = passed
            receipt["release_raw_limit"] = RELEASE_RAW_LIMIT
            receipt["release_raw_pass"] = matching["raw_error"] <= RELEASE_RAW_LIMIT
            receipt["historical_matching_raw_limit"] = .001
        elif timing:
            schema = {"qkv-resource20": "airk.private.compact-fp16-unprofiled-resource20/1",
                      "qkv-fit-probe": "airk.private.compact-fp16-unprofiled-fit/1",
                      "qkv-mixed": "airk.private.compact-fp16-unprofiled-mixed/1"}[args.mode]
            validate_native_identity(value, schema, receipt["owned"], request, evidence, evidence_value, manifest, image, base)
            require(value["profiling_enabled"] is False and value["runtime_profile"] is None
                    and not any(out.glob("ort-profile*")) and value["projection_stage"] == core(stage_pin)
                    and value["projection_reference"] == core(projection_reference)
                    and value["timing_score_sources"] == [core(short), core(sealed_pins[1])]
                    and value["native_forward_returned"] is value["forward_synchronized"] is True
                    and value["numeric_forward_attempts"] == len(value["rows"]), "unprofiled score custody differs")
            policy = BOUNDED_POLICY
            if args.mode == FULL_CUT:
                policy = b"airk.private.compact-full-fp32-cuda-t1014-cuts/1"
                validate_full_cut(value, base, args.full_cut_group)
            validate_development_placement(value, reviewed_placement, graph, sources[BOUNDED_SOURCE], policy=policy)
            events = read_operations(paths["operations"])
            validate_development_operations(events, value, component, args.mode)
            receipt["timing"] = timing_owner["timing_rows"](args.mode, value["rows"], sealed_rows)
            receipt["flash_attention_records"] = development_attention_logs(paths["stdout"].read_bytes(), value["rows"], args.mode)
            timing_owner["same_adapter"](receipt["before_load_adapter"], receipt["after_release_adapter"], value)
            receipt["memory_by_cycle"] = timing_owner["memory_coverage"](
                args.mode, receipt["owned"]["before_GO"]["wddm"], events, value["pid"], luid)
        elif args.mode in WIDE_CAPTURES:
            require(value is not None and "profile" in paths, "native capture or actual profile missing")
            receipt["flash_attention_records"] = validate_wider_capture(
                value, paths, receipt["owned"], request, evidence, evidence_value, manifest, image, component, base,
                projection, projection_reference, graph, reviewed_placement, sources[BOUNDED_SOURCE], sealed_t1024, args.mode,
            )
            receipt["raw_scores_equal_to_sealed"] = True
        elif args.mode == DEVELOPMENT_MODE:
            require(value is not None and "profile" in paths, "native development result or actual profile missing")
            receipt["flash_attention_records"] = validate_development(
                value, paths, receipt["owned"], request, evidence, evidence_value, manifest, image, component, base,
                projection, projection_reference, development_reference, graph, reviewed_placement, sources[BOUNDED_SOURCE],
            )
            receipt["development_screen_pass"] = value["development_screen_pass"]
        else:
            require(value is not None and "profile" in paths, "native T32 result or actual profile trace missing")
            receipt["flash_attention_records"] = validate_t32(
                value, paths, receipt["owned"], request, evidence, evidence_value, manifest, image, component, base,
                mode=args.mode, reviewed=graph, reviewed_placement=reviewed_placement,
                policy_source=sources.get(BOUNDED_SOURCE), projection=projection,
                projection_reference=projection_reference,
            )
        for pin in inputs:
            base.pinned(pin)
        for key, path in paths.items():
            receipt["outputs"][key] = base.pin(path)
        require(time.monotonic() < expires, "original Compact phase deadline expired")
        receipt["complete"] = True
    except BaseException as error:
        receipt["failure"] = {"type": type(error).__name__, "message": str(error)}
        if hasattr(error, "owned_cleanup"):
            receipt["actual_cleanup_failure"] = error.owned_cleanup
    finally:
        finished_utc = datetime.now(timezone.utc)
        finished = time.monotonic()
        receipt["finished_utc"] = finished_utc.isoformat()
        receipt["elapsed_seconds"] = finished - started
        receipt["whole_phase_expired"] = finished >= expires
        if receipt["whole_phase_expired"]:
            receipt["complete"] = False
            receipt["failure"] = receipt["failure"] or {"type": "TimeoutError", "message": "original Compact phase deadline expired"}
        if not receipt["complete"]:
            retained = list(out.rglob("*"))
            try:
                if retained:
                    audit(retained, deadline=None)
                receipt["failure_outputs"] = []
                for path in retained:
                    if path.is_file():
                        limit = (component["DIAGNOSTIC_PROOFS"]["native_result"]
                                 if args.mode not in WIDE_CAPTURES and path == paths["native"]
                                 and args.mode in (*GRAPH_MODES, *TAP_SCOPES, *PROJECTION_SCORE_MODES) else LOG_CAP)
                        size = path.stat().st_size
                        pin = (base.pin(path) if size <= limit else
                               {"path": str(path), "bytes": size, "digest_refused_beyond_bound": True})
                        receipt["failure_outputs"].append(pin)
            except BaseException as error:
                receipt["retention_audit_error"] = str(error)
        component["write_new"](guard, receipt_path, receipt)
    return receipt["complete"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=tuple(SELECTORS))
    for name in ("request", "evidence", "image", "build-receipt", "powershell"):
        parser.add_argument("--" + name, type=Path, required=True)
        parser.add_argument("--" + name + "-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--graph-receipt", type=Path)
    parser.add_argument("--graph-receipt-sha256")
    parser.add_argument("--projection-stage", type=Path)
    parser.add_argument("--projection-stage-sha256")
    parser.add_argument("--projection-reference", type=Path)
    parser.add_argument("--projection-reference-sha256")
    parser.add_argument("--retained-cargo-proof", type=Path)
    parser.add_argument("--retained-cargo-proof-sha256")
    parser.add_argument("--source-snapshot", type=Path)
    parser.add_argument("--observed-reference", type=Path)
    parser.add_argument("--observed-reference-sha256")
    parser.add_argument("--full-cut-group", choices=tuple(FULL_CUT_GROUPS))
    parser.add_argument("--cal-ordinal", type=int, choices=range(100))
    for name in ("preflight", "original-compiled", "references"):
        parser.add_argument("--cal-" + name, type=Path)
        parser.add_argument("--cal-" + name + "-sha256")
    args = parser.parse_args()
    require((args.full_cut_group is not None) == (args.mode == FULL_CUT), "full cut group and selector must agree")
    cal_options = [args.cal_ordinal, args.cal_preflight, args.cal_preflight_sha256, args.cal_original_compiled,
                   args.cal_original_compiled_sha256, args.cal_references, args.cal_references_sha256]
    if (args.mode == CAL_MODE and any(v is None for v in cal_options)) or (args.mode != CAL_MODE and any(v is not None for v in cal_options)):
        parser.error("scientific CAL requires ordinal and all three independently pinned inputs")
    if (args.retained_cargo_proof is None) != (args.retained_cargo_proof_sha256 is None):
        parser.error("retained Cargo lineage requires the exact prior closure and its SHA256")
    if args.mode in (OBSERVED_MODE, DOWN_SCORE, *FULL_OBSERVED):
        if args.observed_reference is None or args.observed_reference_sha256 not in OBSERVED_REFERENCES:
            parser.error("observed development requires one exact reviewed original reference")
    elif args.observed_reference is not None or args.observed_reference_sha256 is not None:
        parser.error("an observed reference requires the observed development mode")
    if args.mode in PROJECTION_MODES:
        expected = FULL_STAGE_SHA if args.mode in FULL_MODES else DOWN_STAGE_SHA if args.mode in DOWN_MODES else QKVO_STAGE_SHA if args.mode in QKVO_MODES else PROJECTION_STAGE_SHA
        if args.projection_stage is None or args.projection_stage_sha256 != expected:
            parser.error(f"{args.mode} requires --projection-stage and the exact reviewed --projection-stage-sha256")
    elif args.projection_stage is not None or args.projection_stage_sha256 is not None:
        parser.error("--projection-stage and --projection-stage-sha256 require a Q/K/V projection mode")
    if args.mode in PROJECTION_SCORE_MODES and args.mode != CAL_MODE:
        expected = (FULL_REFERENCE_SHA if args.mode in FULL_OBSERVED else DOWN_REFERENCE_SHA if args.mode == DOWN_SCORE else DEVELOPMENT_REFERENCE_SHA if args.mode in WIDER_REFERENCE_MODES else
                    QKVO_REFERENCE_SHA if args.mode == "qkvo-t32" else PROJECTION_REFERENCE_SHA)
        if args.projection_reference is None or args.projection_reference_sha256 != expected:
            parser.error(f"{args.mode} requires --projection-reference and the exact --projection-reference-sha256")
    elif args.projection_reference is not None or args.projection_reference_sha256 is not None:
        parser.error("--projection-reference and --projection-reference-sha256 require projection-t32")
    if args.mode in REVIEWED_MODES:
        expected = (FULL_GRAPH_RECEIPT_SHA if args.mode in FULL_OBSERVED else DOWN_GRAPH_RECEIPT_SHA if args.mode == DOWN_SCORE else QKVO_REVIEWED_RECEIPT_SHA if args.mode == "qkvo-t32" else
                    PROJECTION_REVIEWED_RECEIPT_SHA if args.mode in PROJECTION_SCORE_MODES else REVIEWED_RECEIPT_SHA)
        if args.graph_receipt is None or args.graph_receipt_sha256 != expected:
            parser.error(f"{args.mode} requires --graph-receipt and the exact reviewed --graph-receipt-sha256")
    elif args.graph_receipt is not None or args.graph_receipt_sha256 is not None:
        parser.error("--graph-receipt and --graph-receipt-sha256 require a reviewed graph mode")
    raise SystemExit(0 if run(args) else 1)


if __name__ == "__main__":
    main()
