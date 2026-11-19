# SPDX-License-Identifier: Apache-2.0
"""Run the owned Windows Large FP32 placement, scoring or Resource20 experiment."""

import argparse
import ctypes
import datetime
import json
import math
import os
from pathlib import Path
import runpy
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
HOST_BYTES = 48 << 30
PROFILE = "eb467966e528b3b34f0c95d26bdcb0e3f404267b5dcaa673cc6c56755b85905b"
REQUEST_SHA = "9a7daee2c118030c979852efc6780f011356af320e0c3fe0a6d1d9a2a0ab33b0"
BATCH_VIEW_GRAPH_SHA = "5c9a26693eec8522bbfb4d5c4445fa0b60684449a5ea2e1098ec71cca96405b1"
BATCH_VIEW_PREPARATION_SHA = "ea6ebdadacf0b776e00281172edc4c756ee396ae03eb8db0596c420218315039"
BATCH_VIEW_OBSERVATION_SHA = "d4ccefff117a11c68b92c3f9c08b145a4fbb5a470114af5a23107c73a26bc431"
REFERENCE_EQUIVALENCE_SHA = "404a486500b1c3955cc5616554ccf2f7c55cd4855981d4477265688db41278d1"
REFERENCE_BASIS = "equivalent_predecessor"
SCORING_EXECUTION_SHA = "ea9aefbf7e9cefb91bca8384924237f95655131cb49ac571e6900e55f3d6d820"
DEVICE_BUDGET = 44 << 30
RESOURCE_WALL_SECONDS = 3600
RESOURCE_PROOFS = {"native_result": 16 << 20, "operations": 128 << 10, "stdout": 64 << 20}
CPU = "CPUExecutionProvider"
CUDA = "CUDAExecutionProvider"
PARENT_CPU_STATIC_BYTES = 43819
SELECTOR = (
    "inference_engine::pool::arbiter::qualification::software_v2::"
    "cuda_qualification::cuda_large_fp32_load_only_graph_metadata_diagnostic"
)
SCORING_SELECTOR = (
    "inference_engine::pool::arbiter::qualification::software_v2::"
    "cuda_qualification::bounded_hybrid::cuda_large_fp32_bounded_hybrid_profile"
)
BATCH_VIEW_SCORING_SELECTOR = (
    "inference_engine::pool::arbiter::qualification::software_v2::"
    "cuda_qualification::bounded_hybrid::cuda_large_fp32_batch_view_bounded_hybrid_profile"
)
RESOURCE_SELECTOR = (
    "inference_engine::pool::arbiter::qualification::software_v2::"
    "cuda_qualification::bounded_hybrid::cuda_large_fp32_batch_view_bounded_hybrid_resource20"
)
SCORING_INPUTS = {
    "reviewed_graph": ("GRAPH", "1892e5b040d8c51969f1d3361d0929950fa98f0ef4079060e48d00458635488a"),
    "retained_fixture": ("RETAINED", "c87c4edcdec2ab5e98b41bfc186c3bb7d60791e7b9efd0aea186b731eecc3dd2"),
    "supplemental_fixture": ("SUPPLEMENTAL", "020669f0c44a959a0d95798eb3cc52925513e7e549496368017e0827a398aaef"),
}


def candidate_inputs(path, expected_sha, request, probe, base, guard, require):
    guard([path], True)
    pins = {"metadata": base.pin(path)}
    require(pins["metadata"]["sha256"] == expected_sha, "owned Large batch-view metadata digest differs")
    metadata = base.read(path)
    require(metadata["schema"] == "airk.private.large-fp32-batch-view-metadata/1"
            and metadata["execution_user"] == base.SID
            and metadata["qualified"] is metadata["model_requests_dispatched"] is False
            and metadata["parent_request"]["sha256"] == REQUEST_SHA
            and metadata["graph"]["sha256"] == BATCH_VIEW_GRAPH_SHA
            and metadata["preparation"]["sha256"] == BATCH_VIEW_PREPARATION_SHA
            and metadata["probe_request"] == {key: request[key] for key in ("path", "sha256")},
            "owned Large batch-view metadata does not bind the reviewed graph and selected request")
    receipts = {key: metadata[key] for key in
                ("parent_request", "graph", "preparation", "profile_pin", "source_manifest")}
    receipts["native_input"] = probe["gpu_input"]
    guard([Path(pin["path"]) for pin in receipts.values()], True)
    for label, pin in receipts.items():
        observed = base.pin(Path(pin["path"]))
        require(pin == {key: observed[key] for key in ("path", "sha256")},
                "owned Large batch-view evidence differs: " + label)
        pins[label] = observed
    parent_probe = base.read(pins["parent_request"]["path"])
    require(all(probe[key] == parent_probe[key] for key in
                ("schema", "runtime_manifest", "bounds", "cold_bytes", "warm_bytes",
                 "headroom_bytes", "job_limit_bytes")),
            "Large batch-view request changed the original runtime or probe bounds")
    native_input = base.read(pins["native_input"]["path"])
    profile = base.read(pins["profile_pin"]["path"])
    models = [asset for asset in profile["assets"] if asset["role"] == "model"]
    require(native_input["profile"] == metadata["profile_pin"]
            and native_input["source_manifest"] == metadata["source_manifest"]
            and profile["manifest"] == metadata["profile"]
            and profile["device_execution"]["graph"] == BATCH_VIEW_GRAPH_SHA
            and len(models) == 1 and models[0]["sha256"] == BATCH_VIEW_GRAPH_SHA
            and native_input["files"][models[0]["name"]] == metadata["graph"]["path"],
            "Large batch-view profile does not bind the reviewed graph")
    return metadata, pins


def observe_candidate_graph(metadata, summarize):
    rows = metadata["rows"]
    summary = summarize(metadata)
    expected = {f"/scorer/layers.{index}/self_attn/Add_2" for index in range(36)}
    nodes = {(row["graph"], row["node"]): row for row in rows if row["kind"] == "node"}
    values = {(row["graph"], row["value"]): row for row in rows if row["kind"] == "value"}
    mask_adds = {key: {**row, "edges": []} for key, row in nodes.items() if row["name"] in expected}
    cpu_implicit = 0
    for edge in rows:
        if edge["kind"] != "edge" or edge["node"] is None:
            continue
        graph, node, value = edge["graph"], edge["node"], edge["value"]
        key = (graph, node)
        if key in mask_adds:
            mask_adds[key]["edges"].append({
                "role": edge["role"], "slot": edge["slot"], "value": values.get((graph, value)),
            })
        if edge["role"] == 1 and nodes[key]["ep"] == CPU:
            cpu_implicit += 1
    graphs = summary["graphs"]
    cpu_bytes = sum(graph["cpu_incident_static_shapes"]["sum_fixed_payload_bytes"] for graph in graphs)
    return {
        "node_count": summary["node_count"],
        "provider_node_counts": summary["provider_node_counts"],
        "graph_count": len(graphs),
        "root_graph_count": sum(graph["parent_graph"] is None for graph in graphs),
        "cpu_implicit_edges": cpu_implicit,
        "cpu_incident_static_shapes": [graph["cpu_incident_static_shapes"] for graph in graphs],
        "cpu_static_bytes": cpu_bytes,
        "predecessor_cpu_static_bytes": PARENT_CPU_STATIC_BYTES,
        "within_predecessor_cpu_static_bytes": cpu_bytes <= PARENT_CPU_STATIC_BYTES,
        "mask_adds": sorted(mask_adds.values(), key=lambda row: row["name"]),
        "missing_mask_adds": sorted(expected - {row["name"] for row in mask_adds.values()}),
    }


def validate_candidate_graph(observed, require):
    require(observed["graph_count"] == observed["root_graph_count"] == 1
            and set(observed["provider_node_counts"]) <= {CPU, CUDA}
            and observed["cpu_implicit_edges"] == 0,
            "Large batch-view final graph scopes, providers or CPU implicit edges differ")
    shapes = observed["cpu_incident_static_shapes"][0]
    require(shapes["other_or_dynamic_values"] == 0 and set(shapes["rank_counts"]) <= {0, 1}
            and shapes["max_fixed_dim"] <= 5 and shapes["max_fixed_payload_bytes"] <= 40,
            "Large batch-view CPU incident values must be fixed INT64/BOOL within 40 B")
    adds = observed["mask_adds"]
    require(len(adds) == 36 and not observed["missing_mask_adds"],
            "exactly 36 Large batch-view mask Add nodes required")
    for row in adds:
        edges = row["edges"]
        require(row["ep"] == CUDA and row["domain"] in ("", "ai.onnx") and row["op"] == "Add"
                and len(edges) == 3
                and {(edge["role"], edge["slot"]) for edge in edges} == {(0, 0), (0, 1), (2, 0)}
                and all(edge["value"] is not None and edge["value"]["type_state"] == "tensor"
                        and edge["value"]["rank"] == 3 and edge["value"]["element_type"] == 1
                        for edge in edges),
                "Large batch-view mask Add is missing CUDA FP32 rank-three operands or result: "
                + row["name"])


def scoring_inputs(paths, base, guard, require, *, candidate=False):
    guard(paths, True)
    pins = {}
    for (label, (_variable, digest)), path in zip(SCORING_INPUTS.items(), paths):
        expected = BATCH_VIEW_OBSERVATION_SHA if candidate and label == "reviewed_graph" else digest
        pin = base.pin(path)
        require(pin["sha256"] == expected, "reviewed Large input differs: " + label)
        pins[label] = pin
    fixtures = [base.read(pin["path"]) for label, pin in pins.items() if label.endswith("_fixture")]
    producers = [fixture["reference_producer"] for fixture in fixtures]
    guard([Path(pin["path"]) for pin in producers], True)
    for label, producer in zip(("retained_producer", "supplemental_producer"), producers):
        pin = base.pin(producer["path"])
        require(pin["sha256"] == producer["sha256"], "independent Large producer differs")
        pins[label] = pin
    validators = runpy.run_path(str(ROOT / "scripts/cuda-component-execution/bounded_hybrid.py"))
    return pins, validators["selected_rows"](*fixtures), validators["journal"]


def candidate_scoring_references(path, expected_sha, metadata, candidate_pins, scoring_pins, base, guard, require):
    guard([path], True)
    proof = base.pin(path)
    require(proof["sha256"] == expected_sha == REFERENCE_EQUIVALENCE_SHA,
            "reviewed Large batch-view reference equivalence differs")
    lineage = base.read(path)
    require(lineage["schema"] == "airk.private.large-fp32-batch-view-reference-lineage/1"
            and lineage["comparison_basis"] == REFERENCE_BASIS
            and lineage["predecessor_graph"] == "f8ba5d3ea8a4269739ec5e517e61e4ec1f2f94560ea60014a4f5a9476c32fd94"
            and lineage["candidate_graph"] == metadata["graph"]["sha256"]
            and lineage["exact_inverse"] is lineage["operand_order_unchanged"] is True
            and lineage["verified_mask_adds"] == 36 and lineage["native_scoring_dispatched"] is False,
            "Large batch-view reference lineage does not establish equivalent predecessor comparison")
    graph = base.read(scoring_pins["reviewed_graph"]["path"])
    require(graph["request"] == metadata["probe_request"] and graph["profile"] == metadata["profile"]
            and graph["source_manifest"] == metadata["source_manifest"]
            and graph["model_requests_dispatched"] is graph["qualification"] is False,
            "reviewed Large batch-view observation differs from the selected candidate")
    return {"reference_request": candidate_pins["parent_request"], "reference_equivalence": proof}


def validate_scoring(native, owned, pins, rows, journal, out, base, guard, require, *, candidate=False):
    schema = ("airk.private.cuda-large-batch-view-bounded-hybrid-profile/1" if candidate
              else "airk.private.cuda-large-bounded-hybrid-profile/1")
    require(native["schema"] == schema
            and native["complete"] is True and native.get("error") is None
            and native["qualified"] is native["product_admission"] is native["process_cuda_poisoned"] is False
            and native["native_host_reservation_bytes"] == HOST_BYTES
            and 0 < native["peak_process_rss_bytes"] <= HOST_BYTES
            and native["native_control_retained_for_unknown_cuda"] is False
            and native["session_teardown_observed"] is native["native_control_cleanup_observed"] is True
            and native["pid"] == owned["owned_leader_pid"],
            "Large scoring, host reservation or teardown proof differs")
    for label, pin in pins.items():
        require(native[label] == {key: pin[key] for key in ("path", "sha256")},
                "native Large evidence pin differs: " + label)
    if candidate:
        require(native["reference_basis"] == REFERENCE_BASIS,
                "native Large batch-view scoring did not use equivalent predecessor references")
    node_count = 3643 if candidate else 3570
    reference_field = REFERENCE_BASIS if candidate else "same_artifact"
    require(len(native["rows"]) == 4
            and [run["tokens"] for run in native["profiled_runs"]] == [32, 128, 1024, 8192]
            and all(run["node_count"] == node_count and run["cpu_nodes"] == 1663
                    and run["cpu_max_tensor_bytes"] <= 40 for run in native["profiled_runs"]),
            "four complete ordered Large forward traces required")
    for expected, observed in zip(rows, native["rows"]):
        require(observed["tokens"] == len(expected["input"]["input_ids"])
                and observed["name"] == expected["name"]
                and observed["input_digest"] == expected["input"]["input_digest"]
                and len(observed["raw_scores"]) == 8
                and all(math.isfinite(score) for score in observed["raw_scores"])
                and observed["argmax_matches"] is True
                and all(0 <= observed[key] <= limit for key, limit in (
                    (f"{reference_field}_raw_error", .001), ("original_fp32_raw_error", .001),
                    (f"{reference_field}_mass_error", .05), ("original_fp32_mass_error", .05))),
                "Large score rows differ from reviewed input or acceptance")
    trace = Path(native["runtime_profile"]["path"])
    operations = out / "native-result.operations.jsonl"
    require(trace.parent == out and trace.name.startswith("ort-profile")
            and trace.suffix == ".json" and list(out.glob("ort-profile*.json")) == [trace],
            "native trace escaped the guarded Large profile prefix")
    guard([operations, trace], True)
    trace_pin, operation_pin = base.pin(trace), base.pin(operations)
    require(native["runtime_profile"] == trace_pin and 0 < trace_pin["bytes"] <= 64 << 20
            and 0 < operation_pin["bytes"] <= 1 << 20,
            "native trace or operation journal exceeded its retained proof")
    with operations.open(encoding="utf-8") as stream:
        journal([json.loads(line) for line in stream], native)
    return trace_pin, operation_pin


def pinned_core(pin):
    return {key: pin[key] for key in ("path", "sha256")}


def resource_inputs(path, request, candidate, scoring, base, guard, require):
    guard([path], True)
    execution = base.pin(path)
    require(execution["sha256"] == SCORING_EXECUTION_SHA, "retained Large scoring receipt differs")
    observed = base.read(path)
    require(observed["complete"] is True and observed["failure"] is None
            and observed["schema"] == "airk.private.owned-large-cuda-batch-view-scoring/1"
            and observed["request"] == request and observed["candidate_inputs"] == candidate
            and observed["scoring_inputs"] == scoring and observed["reference_basis"] == REFERENCE_BASIS,
            "retained Large scoring does not bind the selected corrected inputs")
    result = observed["native_result"]
    guard([Path(result["path"])], True)
    base.pinned(result)
    native = base.read(result["path"])
    return execution, result, bytes(native["adapter_before"]["uuid"])


class DeviceMemory:
    """Independently sample adapter-wide dedicated framebuffer memory with NVML."""

    INTERVAL_NS = 250_000_000

    class Reading(ctypes.Structure):
        _fields_ = [(key, ctypes.c_ulonglong) for key in ("total", "free", "used")]

    def __init__(self, uuid, require):
        from uuid import UUID

        self.uuid = "GPU-" + str(UUID(bytes=uuid))
        self.require = require
        self.module = Path(os.environ["SystemRoot"]) / "System32/nvml.dll"
        self.api = ctypes.WinDLL(str(self.module))
        prototypes = {
            "nvmlInit_v2": [],
            "nvmlShutdown": [],
            "nvmlDeviceGetHandleByUUID": [ctypes.c_char_p, ctypes.POINTER(ctypes.c_void_p)],
            "nvmlDeviceGetMemoryInfo": [ctypes.c_void_p, ctypes.POINTER(self.Reading)],
        }
        for name, arguments in prototypes.items():
            function = getattr(self.api, name)
            function.argtypes, function.restype = arguments, ctypes.c_int
        self.call("nvmlInit_v2")
        try:
            self.handle = ctypes.c_void_p()
            self.call("nvmlDeviceGetHandleByUUID", self.uuid.encode("ascii"), ctypes.byref(self.handle))
            self.baseline = self.read()
        except BaseException:
            self.call("nvmlShutdown")
            raise
        self.minimum_free = self.baseline
        self.maximum_used = self.baseline
        self.next_sample_ns = self.baseline["monotonic_ns"]
        self.previous_sample_ns = self.baseline["monotonic_ns"]
        self.sample_count = self.maximum_gap_ns = 0

    def call(self, name, *args):
        status = getattr(self.api, name)(*args)
        self.require(status == 0, f"{name} failed with NVML status {status}")

    def read(self):
        value = self.Reading()
        self.call("nvmlDeviceGetMemoryInfo", self.handle, ctypes.byref(value))
        self.require(0 < value.total and 0 <= value.free <= value.total and 0 <= value.used <= value.total,
                     "invalid NVML dedicated memory observation")
        return {"monotonic_ns": time.monotonic_ns(), "total_bytes": value.total,
                "free_bytes": value.free, "used_bytes": value.used}

    def observe_if_due(self, now):
        if now < self.next_sample_ns:
            return
        row = self.read()
        self.sample_count += 1
        self.maximum_gap_ns = max(self.maximum_gap_ns, row["monotonic_ns"] - self.previous_sample_ns)
        self.previous_sample_ns = row["monotonic_ns"]
        self.next_sample_ns = row["monotonic_ns"] + self.INTERVAL_NS
        if row["free_bytes"] < self.minimum_free["free_bytes"]:
            self.minimum_free = row
        if row["used_bytes"] > self.maximum_used["used_bytes"]:
            self.maximum_used = row

    def close(self):
        try:
            return {"adapter_uuid": self.uuid, "provider": "NVML", "scope": "adapter_dedicated_memory",
                    "interval_ns": self.INTERVAL_NS, "maximum_observed_gap_ns": self.maximum_gap_ns,
                    "in_job_observations": self.sample_count, "before_job": self.baseline,
                    "minimum_sampled_free": self.minimum_free, "maximum_sampled_used": self.maximum_used,
                    "after_job": self.read()}
        finally:
            self.call("nvmlShutdown")


def host_preflight(require):
    class MemoryStatus(ctypes.Structure):
        _fields_ = [("length", ctypes.c_uint32), ("load", ctypes.c_uint32)] + [
            (key, ctypes.c_ulonglong) for key in
            ("total_physical", "available_physical", "total_pagefile", "available_pagefile",
             "total_virtual", "available_virtual", "available_extended_virtual")
        ]

    value = MemoryStatus()
    value.length = ctypes.sizeof(value)
    query = ctypes.WinDLL("kernel32", use_last_error=True).GlobalMemoryStatusEx
    query.argtypes = [ctypes.POINTER(MemoryStatus)]
    query.restype = ctypes.c_int
    if not query(ctypes.byref(value)):
        raise ctypes.WinError(ctypes.get_last_error())
    require(min(value.available_physical, value.available_pagefile) >= HOST_BYTES,
            "actual host physical or commit headroom is below the original Large reservation")
    return {"available_physical_bytes": value.available_physical,
            "available_commit_bytes": value.available_pagefile}


def validate_resource(native, owned, row, expected, journal_path, require):
    require(native["schema"] == "airk.private.cuda-large-batch-view-bounded-hybrid-resource/1"
            and native["complete"] is True and native["error"] is None
            and native["qualified"] is native["product_admission"] is native["profiling_enabled"] is False
            and native["process_cuda_poisoned"] is native["native_control_retained_for_unknown_cuda"] is False
            and native["native_control_cleanup_error"] is None
            and all(native[key] is True for key in ("session_teardown_observed",
                    "native_control_cleanup_observed", "native_host_charge_release_observed"))
            and native["tokens"] == 8192 and native["input_digest"] == row["input"]["input_digest"]
            and native["native_host_reservation_bytes"] == HOST_BYTES
            and native["capacity_policy"] is None
            and native["applied_capacity"] == {"arena_limit_bytes": 40 << 30,
                                              "warm_extra_bytes": 8 << 30, "headroom_bytes": 4 << 30}
            and native["reference_basis"] == REFERENCE_BASIS
            and native["original_whole_expires_monotonic_ns"] == expected["whole_ns"]
            and native["operation_deadline_ms"] == 120_000
            and Path(native["operation_journal"]) == journal_path
            and native["resource_policy"] == expected["native_input"]["policy"]
            and native["resource_policy_digest"] == expected["native_input"]["policy_digest"]
            and native["source_manifest"] == expected["native_input"]["source_manifest"]
            and isinstance(native["policy_identity"], str)
            and len(native["policy_identity"]) == 64 and native["policy_identity"] != "0" * 64
            and all(char in "0123456789abcdef" for char in native["policy_identity"])
            and all(native[key] == pinned_core(pin) for key, pin in expected["pins"].items()),
            "native corrected Large Resource20 custody, input, capacity or teardown differs")
    series, warm = native["resource"], native["warmup"]
    samples = series["samples"]
    elapsed = [sample["elapsed_us"] for sample in samples]
    times = [native["load_us"], warm["elapsed_us"], *elapsed]
    rss = [native["peak_process_rss_bytes"], warm["peak_process_rss_bytes"],
           *(sample["peak_process_rss_bytes"] for sample in samples)]
    require(series["complete"] is True and series["failure"] is None and series["attempted_samples"] == len(samples) == 20
            and series["reservation_bytes"] == HOST_BYTES and warm["returned"] is warm["success"] is True
            and all(type(value) is int and 0 < value <= 120_000_000 for value in times)
            and all(type(value) is int and 0 < value <= HOST_BYTES for value in rss)
            and series["nearest_rank_p95_us"] == sorted(elapsed)[18] and series["maximum_us"] == max(elapsed),
            "invalid complete Large Resource20 series")
    observation = owned["before_GO"]["operation_watch"]
    events = observation["observed_events"]
    calls = [native["load_native_call_us"], warm["native_call_elapsed_us"], *native["native_call_samples_us"]]
    require(observation["actual_process_exit_observed"] is True and observation["exit_code"] == 0
            and observation["completed_return_markers"] == len(calls) == 22 and len(events) == 44
            and calls == [event["native_call_elapsed_us"] for event in events[1::2]]
            and all(type(call) is int and 0 < call <= elapsed for call, elapsed in zip(calls, times)),
            "Large Resource20 timing differs from the independent journal")
    with journal_path.open("rb") as stream:
        raw = stream.read(RESOURCE_PROOFS["operations"] + 1)
    require(0 < len(raw) <= RESOURCE_PROOFS["operations"] and raw.endswith(b"\n")
            and [json.loads(line) for line in raw.splitlines()] == events, "final Large journal differs")
    return {"load_us": times[0], "warmup_us": times[1], "samples": len(samples),
            "p95_us": series["nearest_rank_p95_us"], "maximum_us": series["maximum_us"],
            "peak_process_rss_bytes": max(rss)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--request", required=True, type=Path)
    parser.add_argument("--image", required=True, type=Path)
    parser.add_argument("--image-sha256", required=True)
    parser.add_argument("--build-receipt", required=True, type=Path)
    parser.add_argument("--build-receipt-sha256", required=True)
    parser.add_argument("--powershell", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--graph", type=Path)
    parser.add_argument("--retained", type=Path)
    parser.add_argument("--supplemental", type=Path)
    parser.add_argument("--candidate-metadata", type=Path)
    parser.add_argument("--candidate-metadata-sha256")
    parser.add_argument("--reference-equivalence", type=Path)
    parser.add_argument("--reference-equivalence-sha256")
    parser.add_argument("--resource20", action="store_true")
    parser.add_argument("--scoring-execution", type=Path)
    parser.add_argument("--watchdog-sha256")
    args = parser.parse_args()
    resource = args.resource20
    score_paths = (args.graph, args.retained, args.supplemental)
    if any(score_paths) and not all(score_paths):
        parser.error("scoring requires --graph, --retained and --supplemental together")
    scoring = all(score_paths)
    candidate = args.candidate_metadata is not None
    if candidate != (args.candidate_metadata_sha256 is not None):
        parser.error("batch-view mode requires --candidate-metadata and --candidate-metadata-sha256 together")
    candidate_scoring = candidate and scoring
    has_equivalence = args.reference_equivalence is not None
    if has_equivalence != (args.reference_equivalence_sha256 is not None):
        parser.error("reference equivalence requires its path and SHA256 together")
    if candidate_scoring != has_equivalence:
        parser.error("reference equivalence is required for batch-view scoring and is exclusive to that mode")
    if resource and not (candidate_scoring and args.scoring_execution and args.watchdog_sha256):
        parser.error("Resource20 requires corrected scoring inputs, --scoring-execution and --watchdog-sha256")
    if not resource and (args.scoring_execution or args.watchdog_sha256):
        parser.error("scoring execution and watchdog pins require --resource20")

    component = runpy.run_path(str(ROOT / "scripts/cuda-component-execution.py"))
    base = component["base"]()
    require = component["require"]
    require(os.name == "nt" and sys.flags.isolated and sys.flags.dont_write_bytecode,
            "use the verified Windows account and isolated Python -I -B -S")
    base_guard = {"checked_guard": base.pin(ROOT / "scripts/dev/assert-human-execution.ps1"),
                  "powershell": base.pin(args.powershell)}
    parent, owner = base.helpers({"ownership_helper_root": str(base.MAINTAINED_HELPER_ROOT)})
    guard = component["guard_many"]
    mkdir = component["mkdir"]
    request = base.pin(args.request)
    image = base.pin(args.image)
    build = base.pin(args.build_receipt)
    require((candidate or request["sha256"] == REQUEST_SHA) and image["sha256"] == args.image_sha256
            and build["sha256"] == args.build_receipt_sha256,
            "reviewed Large request, staged native image or build receipt differs")
    probe = base.read(args.request)
    manifest = args.image.with_name("airk-cuda-runtime.json")
    manifest_pin = base.pin(manifest)
    require({key: manifest_pin[key] for key in ("path", "sha256")}
            == probe["runtime_manifest"], "native runtime locator or bytes differ")
    observed_build = base.read(args.build_receipt, 32 << 20)
    require(observed_build.get("passed") is True and observed_build.get("inputs_unchanged") is True,
            "the scoped native Cargo build must have passed with stable source")
    log_name = observed_build["log"]
    require(Path(log_name).name == log_name, "the retained Cargo log must be beside its receipt")
    build_log = args.build_receipt.with_name(log_name)
    guard(base_guard, [build_log], True)
    log_pin = base.pin(build_log)
    require(log_pin["bytes"] <= 32 << 20, "Cargo JSON log is oversized")
    with build_log.open(encoding="utf-8") as log:
        artifacts = []
        for line in log:
            if not line.startswith("{"):
                continue
            event = json.loads(line)
            if (event.get("reason") == "compiler-artifact" and event["target"]["name"] == "airk_inference"
                    and event["profile"]["test"] is True and event.get("executable")):
                artifacts.append(Path(event["executable"]))
    require(len(artifacts) == 1, "the Cargo log must contain exactly one Inference lib-test image")
    guard(base_guard, artifacts, True)
    require(base.pin(artifacts[0])["sha256"] == image["sha256"],
            "the staged image differs from the tested Cargo image")
    require(args.output.is_absolute() and args.output.parent.resolve(strict=True) == args.output.parent
            and args.output.is_relative_to(ROOT / "artifacts") and not args.output.exists(),
            "a fresh private campaign output is required")
    guard(base_guard, [args.request, args.image, args.build_receipt, manifest], True)
    require(parent.worker.windows_sid() == base.SID, "parent SID differs")
    def guard_inputs(paths, created):
        return guard(base_guard, paths, created)

    candidate_pins = {}
    scoring_pins = {}
    if candidate:
        metadata, candidate_pins = candidate_inputs(
            args.candidate_metadata, args.candidate_metadata_sha256, request, probe, base, guard_inputs, require,
        )
    if scoring:
        scoring_pins, rows, journal = scoring_inputs(score_paths, base, guard_inputs, require, candidate=candidate)
    if candidate_scoring:
        scoring_pins.update(candidate_scoring_references(
            args.reference_equivalence, args.reference_equivalence_sha256,
            metadata, candidate_pins, scoring_pins, base, guard_inputs, require,
        ))

    resource_pins = {}
    if resource:
        native_root = ("crates/airk-inference/src/inference_engine/pool/arbiter/qualification/"
                       "software_v2/cuda_qualification")
        build_files = observed_build["groups"]["native"]["files"]
        for relative in (f"{native_root}/bounded_hybrid.rs", f"{native_root}/bounded_hybrid/resource20.rs",
                         "crates/airk-retrieval/src/arbiter/runtime/bounded_hybrid.rs"):
            pin = base.pin(ROOT / relative)
            require(build_files.get(relative) == pin["sha256"], "Resource20 native source drifted after link")
            resource_pins[relative] = pin
        execution, result, adapter_uuid = resource_inputs(
            args.scoring_execution, request, candidate_pins, scoring_pins, base, guard_inputs, require,
        )
        watch_path = Path(__file__).with_name("resource20_watch.py")
        watch_pin = base.pin(watch_path)
        require(watch_pin["sha256"] == args.watchdog_sha256, "reviewed Resource20 journal watchdog differs")
        watch_owner = runpy.run_path(str(watch_path))
        base.pinned(watch_pin)
        resource_pins.update(scoring_execution=execution, scoring_result=result, watchdog=watch_pin,
                             launcher=base.pin(Path(__file__)))

    start_ns = time.monotonic_ns()
    start = start_ns / 1e9
    started_utc = datetime.datetime.now(datetime.timezone.utc)
    wall_seconds = RESOURCE_WALL_SECONDS if resource else 120
    whole_ns = start_ns + wall_seconds * 1_000_000_000
    deadline = whole_ns / 1e9
    mkdir(base_guard, args.output, deadline=deadline)
    temp = args.output / "temp"
    mkdir(base_guard, temp, deadline=deadline)
    stdin_path = args.output / "stdin.txt"
    stdout_path = args.output / "stdout-stderr.log"
    native_path = args.output / "native-result.json"
    receipt_path = args.output / "execution.json"
    planned_outputs = [stdin_path, stdout_path, native_path, receipt_path]
    journal_path = args.output / "native-result.operations.jsonl"
    if scoring:
        planned_outputs.append(journal_path)
    if scoring and not resource:
        planned_outputs.append(args.output / "ort-profile")
    guard(base_guard, planned_outputs, deadline=deadline)
    proof_paths = {"native_result": native_path, "operations": journal_path, "stdout": stdout_path}

    selector = SCORING_SELECTOR if scoring else SELECTOR
    schema = ("airk.private.owned-large-cuda-scoring/1" if scoring
              else "airk.private.owned-large-cuda-graph-observation/1")
    if candidate_scoring:
        selector = BATCH_VIEW_SCORING_SELECTOR
        schema = "airk.private.owned-large-cuda-batch-view-scoring/1"
    if resource:
        selector = RESOURCE_SELECTOR
        schema = "airk.private.owned-large-cuda-batch-view-resource20/1"
    command = [str(args.image), "--ignored", "--exact", selector, "--test-threads=1"]
    if resource:
        command.append("--nocapture")
    receipt = {
        "schema": schema,
        "complete": False, "qualification": False,
        "model_requests_dispatched": None if scoring else False,
        "started_utc": started_utc.isoformat(), "wall_seconds": wall_seconds, "host_bytes": HOST_BYTES,
        "command": command, "request": request, "image": image, "build": build, "build_log": log_pin,
        "execution_user_sid": base.SID, "failure": None,
    }
    if candidate or scoring:
        receipt["runtime_manifest"] = manifest_pin
    if candidate:
        receipt["candidate_inputs"] = candidate_pins
    if scoring:
        receipt["scoring_inputs"] = scoring_pins
    if candidate_scoring:
        receipt["reference_basis"] = REFERENCE_BASIS
    if resource:
        receipt.update(resource_inputs=resource_pins, original_deadline_monotonic_ns=whole_ns,
                       original_deadline=(started_utc + datetime.timedelta(seconds=wall_seconds)).isoformat())
    watch = device = None

    def authenticate(api, job, info, child_deadline):
        nonlocal watch
        proof = parent.suspended_authentication(api, info, deadline, child_deadline, base.SID,
                                                image, owner.image_proof)
        require(component["token_owner"](info.process) == base.SID,
                "suspended native default owner differs")
        require(owner.job_snapshot(api, job) == ([int(info.pid)], 1, 1),
                "the suspended native image must be the only Job member")
        proof["default_owner_sid"] = base.SID
        if resource:
            identity = {"input_sha256": request["sha256"], "profile": metadata["profile"],
                        "process_id": int(info.pid), "native_binary_sha256": image["sha256"]}
            state = watch_owner["Operations"](identity, rows[-1]["input"]["input_digest"], whole_ns)
            watch = watch_owner["WindowsWatch"](
                journal_path, stdout_path, native_path, state,
                lambda path: component["guard"](base_guard, path, True, deadline=deadline),
                device=device,
            )
        return proof

    def observe(api, job, info, child_deadline):
        return watch.run(api, job, info, child_deadline)

    try:
        env = {key: value for key, value in os.environ.items()
               if not key.upper().startswith(("AIRK_", "ORT_", "PYTHON", "CUDA_", "CUDNN_"))}
        env.update(
            PATH=r"C:\Windows\System32;C:\Windows",
            TEMP=str(temp), TMP=str(temp), TMPDIR=str(temp),
            AIRK_PRIVATE_CENSUS_BASE=str(temp),
            AIRK_CUDA_QUALIFICATION_INPUT=str(args.request),
            AIRK_CUDA_QUALIFICATION_INPUT_SHA256=request["sha256"],
            AIRK_CUDA_QUALIFICATION_OUTPUT=str(native_path),
        )
        if scoring:
            for label, (variable, _) in SCORING_INPUTS.items():
                env[f"AIRK_CUDA_HYBRID_{variable}"] = scoring_pins[label]["path"]
                env[f"AIRK_CUDA_HYBRID_{variable}_SHA256"] = scoring_pins[label]["sha256"]
            if not resource:
                env["AIRK_CUDA_HYBRID_ORT_PROFILE_PREFIX"] = str(args.output / "ort-profile")
        if candidate_scoring:
            for label in ("reference_request", "reference_equivalence"):
                variable = f"AIRK_CUDA_HYBRID_{label.upper()}"
                env[variable] = scoring_pins[label]["path"]
                env[f"{variable}_SHA256"] = scoring_pins[label]["sha256"]
        if resource:
            env["AIRK_CUDA_HYBRID_WHOLE_EXPIRES_NS"] = str(whole_ns)
            receipt["host_before_job"] = host_preflight(require)
            device = DeviceMemory(adapter_uuid, require)
            require(device.baseline["free_bytes"] >= DEVICE_BUDGET,
                    "actual adapter free memory is below the approved Large device reservation")
        with stdin_path.open("xb"):
            pass
        with stdin_path.open("rb") as stdin, stdout_path.open("xb") as stdout:
            guard(base_guard, [stdin_path, stdout_path], True, deadline=deadline)
            owned = owner.run_windows_owned_training(
                command, ROOT, stdout, deadline - time.monotonic(), env, stdin, HOST_BYTES,
                before_resume=authenticate, after_resume=observe if resource else None,
                absolute_deadline=deadline,
            )
        receipt["owned"] = owned
        if device is not None:
            device, finished_device = None, device
            receipt["device_memory"] = finished_device.close()
        outputs = list(args.output.rglob("*"))
        guard(base_guard, outputs, True, deadline=deadline)
        receipt["stdout"] = base.pin(stdout_path)
        if native_path.exists():
            receipt["native_result"] = base.pin(native_path)
        if scoring:
            if journal_path.exists():
                receipt["operations"] = base.pin(journal_path)
            if not resource:
                receipt["trace_candidates"] = [base.pin(path) for path in args.output.glob("ort-profile*.json")]
        native = base.read(native_path) if native_path.exists() else None
        if owned["exit_code"] != 0 and native and native.get("error"):
            raise RuntimeError(native["error"])
        require(parent.settled(owned) and 0 < owned["peak_host_committed_bytes"] <= HOST_BYTES,
                "native Large exit, Job settlement or host commit proof differs")
        require(native is not None, "native Large result is missing")
        expected_profile = metadata["profile"] if candidate else PROFILE
        require(native["profile"] == expected_profile and native["native_binary_sha256"] == image["sha256"]
                and native["pid"] == owned["owned_leader_pid"]
                and native["execution_user"] == native["default_owner"] == base.SID
                and native["native_host_charge_release_observed"] is True
                and native["actual_job"] == {
                    "process_memory_limit_bytes": HOST_BYTES, "job_memory_limit_bytes": HOST_BYTES,
                    "flags": 0x2308,
                }, "native Large identity, reservation or actual Job differs")
        if scoring:
            require(native["request"] == {key: request[key] for key in ("path", "sha256")}
                    and native["native_input"] == probe["gpu_input"]
                    and native["runtime_manifest"] == probe["runtime_manifest"]
                    and native["bounds"] == probe["bounds"],
                    "native Large scoring did not consume the exact request")
            if candidate_scoring:
                require(native["source_manifest"] == metadata["source_manifest"],
                        "native Large scoring source does not match candidate metadata")
            if resource:
                require({p.name for p in args.output.iterdir()} == {
                    "stdin.txt", "temp", *(p.name for p in proof_paths.values())
                }, "unprofiled Resource20 output roster differs")
                require(receipt["device_memory"]["in_job_observations"] > 0
                        and bytes(native["adapter_before"]["uuid"]) == adapter_uuid
                        and bytes(native["adapter_after"]["uuid"]) == adapter_uuid,
                        "independent dedicated-memory or native adapter evidence differs")
                expected = {
                    "whole_ns": whole_ns, "native_input": base.read(candidate_pins["native_input"]["path"]),
                    "pins": {key: scoring_pins[key] for key in ("reviewed_graph", "retained_fixture",
                                "retained_producer", "reference_request", "reference_equivalence")},
                }
                receipt["resource"] = validate_resource(native, owned, rows[-1], expected,
                                                        journal_path, require)
                receipt["model_requests_dispatched"] = True
            else:
                trace, operations = validate_scoring(
                    native, owned, scoring_pins, rows, journal, args.output, base,
                    lambda paths, created: guard(base_guard, paths, created, deadline=deadline), require,
                    candidate=candidate_scoring,
                )
                receipt.update(runtime_profile=trace, operations=operations, model_requests_dispatched=True)
        else:
            require(native["schema"] == "airk.private.cuda-graph-metadata-diagnostic/1"
                    and native["outcome"] == "graph_metadata_observed" and native["diagnostic_complete"] is True
                    and native["model_requests_dispatched"] is False and native["qualification"] is False,
                    "native Large graph scope differs")
            component["validate_graph_metadata"](native["graph_metadata"], native["placement"])
            if candidate:
                placement = native["placement"]
                require(native["request"] == metadata["probe_request"]
                        and native["native_input"] == probe["gpu_input"]
                        and native["source_manifest"] == metadata["source_manifest"]
                        and native["runtime_manifest"] == probe["runtime_manifest"]
                        and native["bounds"] == probe["bounds"]
                        and native["product_admission"] is native["forward_dispatched"] is False
                        and placement["scope"] == "onnxruntime-final-session-state-ep-placement"
                        and placement["complete"] is True and placement["shim_status"] == 0
                        and placement["ort_error_code"] == 0
                        and placement["reported_nodes"] == len(placement["nodes"]),
                        "native batch-view result does not bind the selected request and complete placement")
                summarize = runpy.run_path(str(Path(__file__).with_name("placement_report.py")))["summarize"]
                observed = observe_candidate_graph(native["graph_metadata"], summarize)
                receipt["candidate_graph_observation"] = observed
                validate_candidate_graph(observed, require)
        outputs = (receipt.get("runtime_profile"), receipt.get("operations"))
        for pin in (request, image, build, log_pin, manifest_pin, receipt["native_result"], receipt["stdout"],
                    *candidate_pins.values(), *scoring_pins.values(), *resource_pins.values(),
                    *(pin for pin in outputs if pin)):
            base.pinned(pin)
        require(time.monotonic() < deadline, "original Large deadline expired")
        receipt["complete"] = True
    except BaseException as error:
        receipt["failure"] = f"{type(error).__name__}: {error}"
        for key in ("emergency_cleanup", "owned_cleanup", "operation_watch"):
            cleanup = getattr(error, key, None)
            if cleanup is not None:
                receipt[key] = cleanup
        raise
    finally:
        if resource:
            prior = sys.exc_info()[1]
            close_errors = []
            for key, observer in (("watchdog", watch), ("device_memory", device)):
                if observer is not None:
                    try:
                        final = observer.close()
                        if final is not None:
                            receipt[key] = final
                    except BaseException as error:
                        close_errors.append(f"{key}: {error}")
            if close_errors:
                receipt["observer_cleanup_errors"] = close_errors
                receipt["failure"] = receipt["failure"] or close_errors[0]
            surviving = [entry for entry in args.output.rglob("*") if entry not in proof_paths.values()]
            try:
                guard(base_guard, [args.output, *surviving], True, deadline=deadline)
            except BaseException as error:
                receipt["failure"] = receipt["failure"] or f"final output custody: {error}"
            receipt["final_guard_errors"], hasher = component["final_output_hasher"](
                base_guard, proof_paths, deadline, base.pin)
            component["finish"](receipt, proof_paths, start, deadline, hasher, proofs=RESOURCE_PROOFS)
            component["write_new"](base_guard, receipt_path, receipt)
            if prior is None and not receipt["complete"]:
                raise ValueError(receipt["failure"])
        else:
            receipt["elapsed_seconds"] = time.monotonic() - start
            with receipt_path.open("x", encoding="utf-8") as stream:
                json.dump(receipt, stream, ensure_ascii=False, indent=2)
                stream.write("\n")
            guard(base_guard, [receipt_path], True)


if __name__ == "__main__":
    main()
