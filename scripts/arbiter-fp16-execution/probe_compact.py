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
import sys
import time


ROOT = Path(__file__).resolve().parents[2]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
PREFIX = "inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::fp16_compact::"
SELECTORS = {
    "metadata": PREFIX + "emit_cuda_compact_fp16_probe_metadata",
    "graph": PREFIX + "cuda_compact_fp16_load_only_graph_diagnostic",
    "t32": PREFIX + "cuda_compact_fp16_t32_component",
    "bounded-t32": PREFIX + "cuda_compact_fp16_bounded_t32_component",
    "layer0-t32": PREFIX + "cuda_compact_fp16_layer0_taps",
    "layer0-projection-t32": PREFIX + "cuda_compact_fp16_layer0_projection_taps",
}
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
REVIEWED_MODES = ("bounded-t32", *TAP_SCOPES)
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
    if mode in TAP_SCOPES:
        relevant += ("crates/airk-retrieval/src/arbiter/runtime/bounded_hybrid/layer0.rs",
                     "crates/airk-retrieval/src/arbiter/runtime/cuda/run.rs")
    sources = {relative: exact_pin(ROOT / relative, native[relative], base) for relative in relevant}
    return log_pin, artifact, sources


def validate_request(mode, request, request_pin, evidence, manifest):
    require(request["schema"] == "airk.private.cuda-component-probe-request/1"
            and request["kind"] == ("prepare" if mode == "metadata" else "probe")
            and request["runtime_manifest"] == core(manifest)
            and request["bounds"] == BOUNDS and request["job_limit_bytes"] == HOST
            and (request["cold_bytes"], request["warm_bytes"], request["headroom_bytes"])
                == (16 << 30, 8 << 30, 2 << 30), "reviewed Compact request, runtime or phase bounds differ")
    if mode == "metadata":
        require(request_pin["sha256"] == PREPARE_SHA, "original reviewed Compact Prepare input required")
        require(evidence["schema"] == INDEPENDENT_SCHEMA and evidence["complete"] is True
                and evidence["model"] == MODEL and evidence["model_inference"] is True
                and evidence["native_execution"] is evidence["qualification"] is False
                and evidence["reference"]["input"]["input_digest"] == T32_DIGEST,
                "reviewed independent Compact candidate/T32 proof required")
    else:
        require(evidence["schema"] == METADATA_SCHEMA and evidence["probe_request"] == core(request_pin)
                and evidence["native_input"] == request["gpu_input"]
                and evidence["runtime_manifest"] == core(manifest), "metadata does not bind this Probe")


def validate_metadata(value, paths, request, evidence, manifest, base):
    require(value["schema"] == METADATA_SCHEMA and value["request"] == core(request)
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


def reviewed_graph(receipt, request, evidence, metadata, manifest, parent, component, base, audit):
    historical = base.read(receipt["path"])
    owned = historical["owned"]
    require(historical["schema"] == "airk.private.owned-compact-fp16-probe/1"
            and historical["mode"] == "graph" and historical["complete"] is True
            and historical["qualification"] is False and historical["failure"] is None
            and historical["whole_phase_expired"] is False and historical["wall_seconds"] == WALL
            and historical["execution_user_sid"] == base.SID and parent.settled(owned)
            and historical["request"] == request and historical["reviewed_evidence"] == evidence
            and historical["runtime_manifest"] == manifest,
            "reviewed owned graph does not bind this Probe, metadata, runtime and settled Job")
    source = historical["outputs"]["native"]
    operations = historical["outputs"]["operations"]
    require(source["sha256"] == REVIEWED_GRAPH_SHA
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
    validate_native_identity(graph, GRAPH_SCHEMA, owned, request, evidence, metadata, manifest,
                             historical["image"], base)
    validate_graph(graph, events, component)
    placement = graph["graph_observation"]["placement"]
    roster = placement_roster(placement)
    require(len(roster) == BOUNDED_TOTAL_NODES and sum(row[-1] == CPU for row in roster) == BOUNDED_CPU_NODES
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


def validate_bounded_t32(value, reviewed, placement, source, *, policy=BOUNDED_POLICY):
    require(value["reviewed_graph"] == core(reviewed)
            and value["bounded_policy_identity"] == bounded_policy_identity(
                reviewed["sha256"], value["profile"], source["sha256"], domain=policy),
            "reviewed graph or bounded FP16 policy identity differs")
    require(placement_roster(value["final_placement"]) == placement_roster(placement),
            "bounded FP16 final placement differs from the complete reviewed graph")
    profiled = value["bounded_profile"]
    expected = {"tokens": 32, "node_count": BOUNDED_TOTAL_NODES, "cpu_nodes": BOUNDED_CPU_NODES}
    bounds = {"model_run_us": (1, value["rows"][0]["native_call_elapsed_us"]),
              "cpu_input_bytes": (0, (1 << 64) - 1), "cpu_output_bytes": (0, (1 << 64) - 1),
              "cpu_max_tensor_bytes": (1, 32)}
    require(isinstance(profiled, dict) and set(profiled) == set(expected) | set(bounds)
            and all(type(profiled[key]) is int and profiled[key] == size for key, size in expected.items())
            and all(type(profiled[key]) is int and lower <= profiled[key] <= upper
                    for key, (lower, upper) in bounds.items()),
            "bounded FP16 profile timing, complete node coverage or CPU tensor bounds differ")


def validate_layer0_evidence(value, component, roster=LAYER0_TAPS):
    taps = value["layer0_taps"]
    require(isinstance(taps, list) and len(taps) == len(roster), "exact four layer-0 taps required")
    for tap, (name, width) in zip(taps, roster):
        require(isinstance(tap, dict) and set(tap) == {"name", "width", "bits"}
                and tap["name"] == name and type(tap["width"]) is int and tap["width"] == width,
                "layer-0 tap name, order or width differs")
        bits = tap["bits"]
        require(isinstance(bits, list) and len(bits) == 32 * width
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
                 mode="t32", reviewed=None, reviewed_placement=None, policy_source=None):
    if mode in TAP_SCOPES:
        schema, policy, roster = TAP_SCOPES[mode]
    else:
        schema = {"t32": T32_SCHEMA, "bounded-t32": BOUNDED_SCHEMA}[mode]
        policy, roster = BOUNDED_POLICY, None
    validate_native_identity(value, schema, owned, request, evidence, metadata, manifest, image, base)
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
        validate_bounded_t32(value, reviewed, reviewed_placement, policy_source, policy=policy)
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
    validate_request(args.mode, request_value, request, evidence_value, manifest)
    log, artifact, sources = linked_image(build, image, base, audit, args.mode)
    dependencies = {}
    if args.mode == "metadata":
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
    graph_receipt = graph = graph_operations = reviewed_placement = None
    if args.mode in REVIEWED_MODES:
        audit([args.graph_receipt])
        graph_receipt = exact_pin(args.graph_receipt, REVIEWED_RECEIPT_SHA, base)
        graph, graph_operations, reviewed_placement = reviewed_graph(
            graph_receipt, request, evidence, evidence_value, manifest, parent, component, base, audit,
        )
        inputs.extend((graph_receipt, graph, graph_operations))
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
    component["mkdir"](guard, out, deadline=expires)
    temp = out / "temp"
    metadata = out / "metadata"
    common = {"stdin": out / "stdin.txt", "stdout": out / "stdout-stderr.log"}
    paths = dict(common, metadata=metadata / "metadata-receipt.json") if args.mode == "metadata" else dict(
        common, native=out / "native-result.json", operations=out / "native-result.operations.jsonl",
    )
    receipt_path = out / "execution.json"
    try:
        component["mkdir"](guard, temp, deadline=expires)
        planned = [receipt_path, *common.values()]
        if args.mode == "metadata":
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
        if args.mode == "metadata":
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
        if args.mode != "metadata" and paths["native"].is_file():
            value = base.read(paths["native"])
            if value.get("error"):
                raise ValueError(value["error"])
        require(exit_code == 0, f"native child exited with Windows status 0x{exit_code:08X}")
        require(parent.settled(receipt["owned"])
                and finite(receipt["owned"]["peak_host_committed_bytes"], 1, HOST),
                "successful native child Job or peak host proof differs")
        if args.mode == "metadata":
            value = base.read(paths["metadata"])
            receipt["metadata_artifacts"] = validate_metadata(value, paths, request, evidence, manifest, base)
        elif args.mode == "graph":
            require(value is not None, "native graph diagnostic result missing")
            validate_native_identity(value, GRAPH_SCHEMA, receipt["owned"], request, evidence,
                                     evidence_value, manifest, image, base)
            validate_graph(value, read_operations(paths["operations"]), component)
        else:
            require(value is not None and "profile" in paths, "native T32 result or actual profile trace missing")
            receipt["flash_attention_records"] = validate_t32(
                value, paths, receipt["owned"], request, evidence, evidence_value, manifest, image, component, base,
                mode=args.mode, reviewed=graph, reviewed_placement=reviewed_placement,
                policy_source=sources.get(BOUNDED_SOURCE),
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
                                 if args.mode in ("graph", *TAP_SCOPES) and path == paths["native"] else LOG_CAP)
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
    args = parser.parse_args()
    if args.mode in REVIEWED_MODES:
        if args.graph_receipt is None or args.graph_receipt_sha256 != REVIEWED_RECEIPT_SHA:
            parser.error(f"{args.mode} requires --graph-receipt and the exact reviewed --graph-receipt-sha256")
    elif args.graph_receipt is not None or args.graph_receipt_sha256 is not None:
        parser.error("--graph-receipt and --graph-receipt-sha256 require a reviewed graph mode")
    raise SystemExit(0 if run(args) else 1)


if __name__ == "__main__":
    main()
