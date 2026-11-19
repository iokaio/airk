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
    "qkv-development": PREFIX + "cuda_compact_fp32_qkv_projection_development_batch",
    "qkv-layer0-t1024": PREFIX + "cuda_compact_fp32_qkv_layer0_t1024",
    "qkvo-metadata": PREFIX + "emit_cuda_compact_fp32_qkvo_projection_probe_metadata",
    "qkvo-graph": PREFIX + "cuda_compact_fp32_qkvo_projection_load_only_graph_diagnostic",
    "qkvo-t32": PREFIX + "cuda_compact_fp32_qkvo_projection_t32_component",
    "t32": PREFIX + "cuda_compact_fp16_t32_component",
    "bounded-t32": PREFIX + "cuda_compact_fp16_bounded_t32_component",
    "layer0-t32": PREFIX + "cuda_compact_fp16_layer0_taps",
    "layer0-projection-t32": PREFIX + "cuda_compact_fp16_layer0_projection_taps",
}
QKVO_MODES = ("qkvo-metadata", "qkvo-graph", "qkvo-t32")
DEVELOPMENT_MODE = "qkv-development"
WIDER_TAP_MODE = "qkv-layer0-t1024"
WIDER_REFERENCE_MODES = (DEVELOPMENT_MODE, WIDER_TAP_MODE)
PROJECTION_SCORE_MODES = ("projection-t32", "qkvo-t32", *WIDER_REFERENCE_MODES)
PROJECTION_MODES = ("projection-metadata", "projection-graph", "projection-t32", *WIDER_REFERENCE_MODES, *QKVO_MODES)
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
DEVELOPMENT_RAW_LIMIT = .010
DEVELOPMENT_NEAR_TIE = .020
METADATA_MODES = ("metadata", "projection-metadata", "qkvo-metadata")
GRAPH_MODES = ("graph", "projection-graph", "qkvo-graph")
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


def job_limits(api, job):
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
    require(actual == JOB and observed.basic.active_limit == 2, "actual suspended 32/32 GiB Job differs")
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


def linked_image(build, image, base, audit, mode):
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
    audit(selected)
    artifact = base.pin(selected[0])
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
    if mode in TAP_SCOPES or mode in ("projection-graph", "qkvo-graph", *PROJECTION_SCORE_MODES):
        relevant += ("crates/airk-retrieval/src/arbiter/runtime/bounded_hybrid/layer0.rs",)
    if mode in TAP_SCOPES or mode == WIDER_TAP_MODE:
        relevant += ("crates/airk-retrieval/src/arbiter/runtime/cuda/run.rs",)
    sources = {relative: exact_pin(ROOT / relative, native[relative], base) for relative in relevant}
    return log_pin, artifact, sources


def has_output_projection(projection):
    return projection is not None and projection[1]["schema"] == QKVO_STAGE_SCHEMA


def projection_schema(projection, qkv_schema):
    return qkv_schema.replace("fp32-qkv-", "fp32-qkvo-") if has_output_projection(projection) else qkv_schema


def projection_stage(path, expected, base, audit, *, output_projection=False):
    stage_sha = QKVO_STAGE_SHA if output_projection else PROJECTION_STAGE_SHA
    graph_sha = QKVO_GRAPH_SHA if output_projection else PROJECTION_GRAPH_SHA
    stage_schema = (QKVO_STAGE_SCHEMA if output_projection
                    else "airk.private.compact-fp32-qkv-projection-candidate/1")
    count = 112 if output_projection else 84
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
            and value["graph_proof"]["added_cast_count"] == 3 * count,
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
        require(request_pin["sha256"] == PREPARE_SHA, "original reviewed Compact Prepare input required")
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
    require(finite(row["matching_fp16_raw_error"], 0, .001)
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


def validate_projection_graph(graph, *, output_projection=False):
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

    widths = {"k": 1024, "v": 1024, "q": 2048}
    if output_projection:
        widths["o"] = 2048
    roster = {f"/scorer/layers.{layer}/self_attn/{name}_proj/MatMul": (layer, width, name)
              for layer in range(28) for name, width in widths.items()}
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

    input_casts = set()
    for row in projections:
        name, index = row["name"], row["node"]
        layer, width, kind = roster[name]
        cuda_node(index, "MatMul", name)
        a32, b32 = typed_values(index, 0, (1, 1), name)
        c32, = typed_values(index, 2, (1,), name)
        stem = "/airk_compact_qkv_fp32" + name
        a16, a_cast = one_cast_source(a32, 10, name)
        b16, b_cast = one_cast_source(b32, 10, name)
        require(b_cast == stem + "/weight32" and b16["provenance"] == "immutable_session_initializer"
                and b16["rank"] == 2 and [dimensions.get((b16["value"], axis)) for axis in (0, 1)] == [2048, width],
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
            require(a_cast in {"/airk_compact_qkv_fp32" + label + "/input32" for label in activation_stems}
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
    return {"cuda_fp32_matmuls": len(projections), "weight_upcasts": len(projections),
            "activation_upcasts": len(input_casts), "output_downcasts": len(projections)}


def reviewed_graph(receipt, request, evidence, metadata, manifest, parent, component, base, audit, *, projection=None):
    historical = base.read(receipt["path"])
    owned = historical["owned"]
    mode = "qkvo-graph" if has_output_projection(projection) else "projection-graph" if projection is not None else "graph"
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
    expected = (QKVO_REVIEWED_GRAPH_SHA if has_output_projection(projection) else
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
                                          output_projection=has_output_projection(projection))
        require(graph.get("projection_stage") == historical.get("projection_stage") == core(pin)
                and historical["projection_assets"] == {name: stage[name] for name in ("candidate_graph", "candidate_weights")}
                and typed == historical["projection_graph"], "reviewed Q/K/V graph stage or typed summary differs")
    placement = graph["graph_observation"]["placement"]
    roster = placement_roster(placement)
    total = (placement["reported_nodes"] if has_output_projection(projection) else
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
    require(isinstance(taps, list) and len(taps) == len(roster), "exact four layer-0 taps required")
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
                           projection, reference_pin, graph, placement, policy_source, sealed):
    validate_native_identity(value, WIDER_TAP_SCHEMA, owned, request, evidence, metadata, manifest, image, base)
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
    validate_bounded_t32(value, graph, placement, policy_source, policy=WIDER_TAP_POLICY,
                         total=PROJECTION_TOTAL_NODES, tokens=WIDER_TAP_TOKENS)
    validate_layer0_evidence(value, component, tokens=WIDER_TAP_TOKENS)
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
    return attention_logs(paths["stdout"].read_bytes(), WIDER_TAP_MODE)


def development_metrics(raw, reference):
    def f32(values):
        return struct.unpack("<8f", struct.pack("<8f", *values))

    def mass(values):
        peak = max(values)
        weights = [math.exp(value - peak) for value in values]
        total = sum(weights)
        return [value / total for value in weights]

    actual, expected = f32(raw), f32(reference)
    order = sorted(range(8), key=expected.__getitem__, reverse=True)
    actual_order = sorted(range(8), key=actual.__getitem__, reverse=True)
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


def development_attention_logs(raw, rows):
    require(0 < len(raw) <= LOG_CAP, "complete bounded development log required")
    prefix = f"test {SELECTORS[DEVELOPMENT_MODE]} ... "
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
    return [attention_logs("\n".join(chunk).encode("utf-8"), DEVELOPMENT_MODE) for chunk in chunks]


def validate_development_operations(events, value, component):
    rows = value["rows"]
    require(len(events) == 2 * (1 + len(rows)), "exact development Load and Numeric journals required")
    component["validate_operations"](events[:2], value, load_only=True)
    identity = events[0]["identity"]
    previous = events[1]["observed_monotonic_ns"]
    for ordinal, row in enumerate(rows, 1):
        start, end = events[2 * ordinal:2 * ordinal + 2]
        for turn, phase in ((start, "started"), (end, "returned")):
            sequence = 2 * ordinal + (1 if phase == "started" else 2)
            require(turn["schema"] == "airk.arbiter.native-operation/1"
                    and turn["sequence"] == sequence and turn["identity"] == identity
                    and turn["kind"] == "numeric" and turn["ordinal"] == ordinal
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


def validate_development(value, paths, owned, request, evidence, metadata, manifest, image, component, base,
                         projection, reference_pin, reference_proof, graph, placement, source):
    validate_native_identity(value, DEVELOPMENT_SCHEMA, owned, request, evidence, metadata, manifest, image, base)
    require(value["development_only"] is True and value["qualification_cutoff_raw"] == .001
            and value["development_raw_limit"] == DEVELOPMENT_RAW_LIMIT
            and value["matching_qkv_mass_limit"] == value["original_fp32_mass_limit"] == .05
            and value["near_tie_margin_limit"] == DEVELOPMENT_NEAR_TIE
            and value["reference_basis"] == "independent_wider_qkv_cpu_approximation"
            and value["reference_recipe"] == reference_proof["recipe"]
            and value["projection_stage"] == core(projection[0]) and value["projection_reference"] == core(reference_pin)
            and value["native_forward_returned"] is value["forward_synchronized"] is True
            and value["numeric_forward_attempts"] == len(DEVELOPMENT_WIDTHS),
            "development scope, cutoffs or comparison identity differs")
    actual_placement = value["final_placement"]
    require(actual_placement["complete"] is True
            and actual_placement["shim_status"] == actual_placement["ort_error_code"] == 0
            and placement_roster(actual_placement) == placement_roster(placement)
            and value["reviewed_graph"] == core(graph)
            and value["bounded_policy_identity"] == bounded_policy_identity(
                graph["sha256"], value["profile"], source["sha256"]),
            "development placement or reviewed graph policy differs")
    plan = value["attention_plan"]
    expected_math = {"gemm": "2", "debug": "1", "unicode": True}
    require(plan["schema"] == "airk.native.cuda-fp16-attention-plan/1"
            and plan["dtype"] == "fp16" and plan["backend"] == "FLASH_ATTENTION"
            and plan["math_environment"] == {"captured_before_ort": expected_math, "current": expected_math}
            and len(plan["nodes"]) == 28 and {node["name"] for node in plan["nodes"]} == NAMES
            and all(node["operator"] == "GroupQueryAttention" and node["domain"] == "com.microsoft"
                    for node in plan["nodes"]), "exact development FP16 FlashAttention plan required")
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
        screened = matching["raw_error"] <= DEVELOPMENT_RAW_LIMIT and (matching["argmax_matches"] or near_tie)
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


def run(args):
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
                                   output_projection=args.mode in QKVO_MODES)
                  if args.mode in PROJECTION_MODES else None)
    validate_request(args.mode, request_value, request, evidence_value, manifest, projection=projection)
    log, artifact, sources = linked_image(build, image, base, audit, args.mode)
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
            elif args.mode in PROJECTION_SCORE_MODES:
                projection_reference = read_projection_reference(
                    args.projection_reference, args.projection_reference_sha256, projection, independent, base, audit,
                )
                inputs.append(projection_reference)
    graph_receipt = graph = graph_operations = reviewed_placement = None
    if args.mode in REVIEWED_MODES:
        audit([args.graph_receipt])
        expected = (QKVO_REVIEWED_RECEIPT_SHA if has_output_projection(projection) else
                    PROJECTION_REVIEWED_RECEIPT_SHA if projection is not None else REVIEWED_RECEIPT_SHA)
        graph_receipt = exact_pin(args.graph_receipt, expected, base)
        graph, graph_operations, reviewed_placement = reviewed_graph(
            graph_receipt, request, evidence, evidence_value, manifest, parent, component, base, audit,
            projection=projection,
        )
        inputs.extend((graph_receipt, graph, graph_operations))
    sealed_t1024 = None
    if args.mode == WIDER_TAP_MODE:
        sealed_pins, sealed_t1024 = read_sealed_t1024(
            request, stage_pin, graph, projection_reference, parent, base, audit,
        )
        inputs.extend(sealed_pins)
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
            if args.mode in SCORING_MODES:
                planned.append(out / "ort-profile")
        audit(planned, False)
        env = {key: value for key, value in os.environ.items()
               if not key.upper().startswith(("AIRK_", "ORT_", "PYTHON", "CUDA_", "CUDNN_"))}
        env.update(PATH=r"C:\Windows\System32;C:\Windows", TEMP=str(temp), TMP=str(temp), TMPDIR=str(temp),
                   AIRK_PRIVATE_CENSUS_BASE=str(temp), CUDA_CACHE_PATH=str(temp),
                   AIRK_CUDA_QUALIFICATION_INPUT=request["path"], AIRK_CUDA_QUALIFICATION_INPUT_SHA256=request["sha256"])
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
            if args.mode in SCORING_MODES:
                env["AIRK_CUDA_FP16_PROFILE_PREFIX"] = str(out / "ort-profile")
            if graph is not None:
                env.update(AIRK_CUDA_FP16_REVIEWED_GRAPH=graph["path"],
                           AIRK_CUDA_FP16_REVIEWED_GRAPH_SHA256=graph["sha256"])
            receipt["inherited_math_environment"] = {key: env[key] for key in
                ("ORT_CUDA_GEMM_OPTIONS", "ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO")}

        def before_resume(api, job, info, child_deadline):
            proof = parent.suspended_authentication(api, info, expires, child_deadline, base.SID,
                                                    image, owner.image_proof)
            require(component["token_owner"](info.process) == base.SID
                    and owner.job_snapshot(api, job) == ([int(info.pid)], 1, 1),
                    "suspended Compact default owner or Job census differs")
            return dict(proof, default_owner_sid=base.SID, job_limits=job_limits(api, job))

        with paths["stdin"].open("xb"):
            pass
        with paths["stdin"].open("rb") as stdin, paths["stdout"].open("xb") as stdout:
            audit(common.values())
            receipt["owned"] = owner.run_windows_owned_training(
                command, ROOT, stdout, expires - time.monotonic(), env, stdin, HOST,
                before_resume=before_resume, absolute_deadline=expires,
            )
        exit_code = observed_native_exit(receipt["owned"])
        observed = list(out.rglob("*"))
        audit(observed)
        require(paths["stdout"].stat().st_size <= LOG_CAP, "actual stdout exceeded its retained bound")
        if args.mode in SCORING_MODES:
            traces = list(out.glob("ort-profile*.json"))
            if len(traces) == 1:
                paths["profile"] = traces[0]
        value = None
        if not metadata_mode and paths["native"].is_file():
            value = (base.read(paths["native"], LOG_CAP) if args.mode == WIDER_TAP_MODE
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
                )
        elif args.mode == WIDER_TAP_MODE:
            require(value is not None and "profile" in paths, "native capture or actual profile missing")
            receipt["flash_attention_records"] = validate_wider_capture(
                value, paths, receipt["owned"], request, evidence, evidence_value, manifest, image, component, base,
                projection, projection_reference, graph, reviewed_placement, sources[BOUNDED_SOURCE], sealed_t1024,
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
                                 if args.mode != WIDER_TAP_MODE and path == paths["native"]
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
    args = parser.parse_args()
    if args.mode in PROJECTION_MODES:
        expected = QKVO_STAGE_SHA if args.mode in QKVO_MODES else PROJECTION_STAGE_SHA
        if args.projection_stage is None or args.projection_stage_sha256 != expected:
            parser.error(f"{args.mode} requires --projection-stage and the exact reviewed --projection-stage-sha256")
    elif args.projection_stage is not None or args.projection_stage_sha256 is not None:
        parser.error("--projection-stage and --projection-stage-sha256 require a Q/K/V projection mode")
    if args.mode in PROJECTION_SCORE_MODES:
        expected = (DEVELOPMENT_REFERENCE_SHA if args.mode in WIDER_REFERENCE_MODES else
                    QKVO_REFERENCE_SHA if args.mode == "qkvo-t32" else PROJECTION_REFERENCE_SHA)
        if args.projection_reference is None or args.projection_reference_sha256 != expected:
            parser.error(f"{args.mode} requires --projection-reference and the exact --projection-reference-sha256")
    elif args.projection_reference is not None or args.projection_reference_sha256 is not None:
        parser.error("--projection-reference and --projection-reference-sha256 require projection-t32")
    if args.mode in REVIEWED_MODES:
        expected = (QKVO_REVIEWED_RECEIPT_SHA if args.mode == "qkvo-t32" else
                    PROJECTION_REVIEWED_RECEIPT_SHA if args.mode in PROJECTION_SCORE_MODES else REVIEWED_RECEIPT_SHA)
        if args.graph_receipt is None or args.graph_receipt_sha256 != expected:
            parser.error(f"{args.mode} requires --graph-receipt and the exact reviewed --graph-receipt-sha256")
    elif args.graph_receipt is not None or args.graph_receipt_sha256 is not None:
        parser.error("--graph-receipt and --graph-receipt-sha256 require a reviewed graph mode")
    raise SystemExit(0 if run(args) else 1)


if __name__ == "__main__":
    main()
