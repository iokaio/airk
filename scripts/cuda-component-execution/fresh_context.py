# SPDX-License-Identifier: Apache-2.0
"""Run a reviewed original FP32 Compact T128 or T8192 component on Windows."""

import argparse
from datetime import datetime, timedelta, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import struct
import sys
import time
import types


sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
SELF = Path(__file__).resolve()
CONTRACT = HERE / "FRESH-CONTEXT.md"
COMPONENT = ROOT / "scripts/cuda-component-execution.py"
NATIVE = ROOT / "scripts/cuda-native-execution.py"
GRAPH = HERE / "fresh_graph.py"
COMPONENT_SHA = "fe47b26e5902372ef27ad7067bc3ba256699dbadbd9abefd04941bd138e0c68c"
NATIVE_SHA = "41b071551b72baae4791b0c5b0ac194cd4699665be519319d753f877f272bc29"
FIXTURE_SHA = "3962186640b42648b345fa200516ed44a3de4ccab0d783e671c9c22c3b8b1bbd"
REFERENCE_SHA = "c48e88b5d6bd8dff20b052bedebf0a4a48354d57caa1d2944b2dda16281805b2"
SCHEMA = "airk.private.cuda-fresh-context-component-plan/1"
PREFIX = "inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::"
ROWS = {
    128: (1, "all-live-1", "791067affc1eeba53660a0044369f22f1f463535656419c5da9a486517222c7a"),
    8192: (2, "native-all-live-8192", "ca9d34d962e308e6e53af8e14101058e8f02fbb4151bcc65eda2ad88be3de1f0"),
}
REFERENCES = ("reference_same_artifact", "reference_original_fp32")
GRAPH_LINKS = (
    "wrapper", "custody_owner", "output_guard", "output_guard_lock", "metadata_receipt",
    "image_packet", "native_build_receipt", "native_image", "request", "native_input",
    "profile", "descriptor", "runtime_root", "runtime_files", "required_native_sources",
)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def selector(tokens):
    return PREFIX + f"cuda_original_context_fp32_t{tokens}_component"


def source(path, expected):
    with path.open("rb") as stream:
        raw = stream.read((2 << 20) + 1)
    require(0 < len(raw) <= 2 << 20 and hashlib.sha256(raw).hexdigest() == expected,
            "reviewed source differs: " + str(path))
    return raw


def private_module(path, raw, name):
    module = types.ModuleType(name)
    module.__file__ = str(path)
    exec(compile(raw, str(path), "exec", dont_inherit=True), module.__dict__)
    return module


def selected_row(native, fixture, tokens):
    index, name, digest = ROWS[tokens]
    require(native["numeric_fixture"]["sha256"] == FIXTURE_SHA
            and fixture["reference_producer"]["sha256"] == REFERENCE_SHA,
            "exact original Compact fixture and independent reference producer required")
    require(fixture.get("schema") == "airk.arbiter.native-numeric/2"
            and fixture.get("model") == "arbiter-context-qwen3-17b",
            "original Compact fixture schema/model differs")
    row = fixture["rows"][index]
    item = row["input"]
    require(row["name"] == name and item["input_digest"] == digest,
            "exact selected Compact input identity required")
    ids = item["input_ids"]
    mask = item["attention_mask"]
    positions = item["position_ids"]
    options = item["candidate_positions"]
    require(len(ids) == len(mask) == len(positions) == tokens
            and all(type(value) is int and value >= 0 for value in ids)
            and all(type(value) is int and value == 1 for value in mask)
            and all(type(value) is int and value == index for index, value in enumerate(positions))
            and type(item["decision_position"]) is int and item["decision_position"] == tokens - 1
            and len(options) == 8 and all(type(value) is int and value >= 0 for value in options)
            and len(item["option_mask"]) == 8 and all(value is True for value in item["option_mask"]),
            "selected Compact row requires original all-live tensor shape and eight candidates")
    for field in REFERENCES:
        require(eight_scores(row.get(field)), "full finite independent Compact references required")
    return row


def eight_scores(scores):
    return isinstance(scores, list) and len(scores) == 8 and all(
        type(score) in (int, float) and math.isfinite(score) for score in scores
    )


def compare(actual, expected):
    # The model and independent reference arrays have the native f32 representation.
    actual = [struct.unpack("<f", struct.pack("<f", value))[0] for value in actual]
    expected = [struct.unpack("<f", struct.pack("<f", value))[0] for value in expected]

    def probability(scores):
        peak = max(scores)
        weights = [math.exp(score - peak) for score in scores]
        total = sum(weights)
        return [weight / total for weight in weights]

    def difference(left, right):
        return max(abs(a - b) for a, b in zip(left, right, strict=True))

    winning = range(len(actual))
    return (difference(actual, expected), difference(probability(actual), probability(expected)),
            max(winning, key=actual.__getitem__) == max(winning, key=expected.__getitem__))


def validate_result(value, plan, pid, native, profile, fixture, component, tokens):
    # The settled native result's common envelope follows the frozen component owner.
    require(value.get("schema") == "airk.private.cuda-component-result/1"
            and value.get("complete") is True and value.get("qualified") is False
            and value.get("product_admission") is False and value.get("measured_peak_device_bytes", "missing") is None,
            "native component success/scope absent")
    base = component.base()
    for key, expected in (("request", plan["request"]), ("native_input", plan["native_input"]),
                          ("runtime_manifest", plan["descriptor"]), ("fixture", native["numeric_fixture"]),
                          ("reference_producer", fixture["reference_producer"]),
                          ("source_manifest", native["source_manifest"])):
        require(base.core_pin_matches(value.get(key), expected), "native capture identity differs: " + key)
    require(value.get("profile") == profile["manifest"] and value.get("model") == "arbiter-context-qwen3-17b"
            and type(value.get("pid")) is int and value["pid"] == pid and value.get("execution_user") == component.SID
            and value.get("native_binary_sha256") == plan["native_image"]["sha256"],
            "actual native PID/image/model differs")
    require(value.get("bounds") == component.BOUNDS and value.get("native_host_reservation_bytes") == 16 << 30,
            "native resource charge differs")
    job = value.get("actual_job", {})
    require(job.get("process_memory_limit_bytes") == 16 << 30 and job.get("job_memory_limit_bytes") == 24 << 30
            and type(job.get("flags")) is int and job["flags"] & 0x2300 == 0x2300, "actual native Job limits differ")
    for key, expected in (("native_control_retained_for_unknown_cuda", False), ("native_control_cleanup_observed", True),
                          ("native_host_charge_release_observed", True), ("process_cuda_poisoned", False),
                          ("native_control_cleanup_error", None), ("final_host_peak_error", None)):
        require(key in value, "missing native final proof: " + key)
        component.exact(value[key], expected, "native final settlement differs: " + key)
    require("error" not in value and value.get("final_report_retention_exceeded_original_deadline", False) is False,
            "native failure retained")
    finite = component.finite
    require(finite(value.get("peak_process_rss_bytes"), 1, 16 << 30)
            and finite(value.get("elapsed_us_to_final_report_start"), 0, 119999999)
            and finite(value.get("load_us"), 0, 120000000), "native original time/RSS proof absent")
    validate_device(value, profile)

    original = selected_row(native, fixture, tokens)
    rows = value.get("rows")
    require(isinstance(rows, list) and len(rows) == 1, "exactly one selected Compact row required")
    row = rows[0]
    require(row.get("name") == original["name"] and type(row.get("tokens")) is int and row["tokens"] == tokens
            and row.get("input_digest") == original["input"]["input_digest"], "selected Compact row identity differs")
    require(eight_scores(row.get("raw_scores")) and row.get("argmax_matches") is True,
            "finite complete Compact scores/argmax required")
    for prefix, field in (("same_artifact", REFERENCES[0]), ("original_fp32", REFERENCES[1])):
        require(finite(row.get(prefix + "_raw_error"), 0, .001)
                and finite(row.get(prefix + "_mass_error"), 0, .05), "original numeric gate refused: " + prefix)
        raw, mass, argmax = compare(row["raw_scores"], original[field])
        require(raw <= .001 and mass <= .05 and argmax, "independent score replay refused: " + prefix)
    require(finite(row.get("elapsed_us"), 0, 120000000)
            and finite(row.get("native_call_elapsed_us"), 0, row["elapsed_us"]), "native row timing absent")


def validate_device(value, profile):
    place = value.get("final_placement", {})
    nodes = place.get("nodes")
    require(place.get("scope") == "onnxruntime-final-session-state-ep-placement" and place.get("complete") is True
            and place.get("shim_status") == 0 and place.get("ort_error_code") == 0
            and isinstance(nodes, list) and 0 < len(nodes) <= 65536 and place.get("reported_nodes") == len(nodes),
            "complete final placement absent")
    total = 0
    for node in nodes:
        require(type(node.get("subgraph_index")) is int and node["subgraph_index"] >= 0
                and node.get("ep") == "CUDAExecutionProvider" and isinstance(node.get("op"), str) and node["op"],
                "unexpected native graph placement")
        for key in ("name", "domain", "op", "ep"):
            require(isinstance(node.get(key), str) and len(node[key].encode()) <= 4096, "placement string bound")
            total += len(node[key].encode())
    require(total == place.get("reported_string_bytes") and total <= 16 << 20, "placement byte coverage differs")
    identity = value.get("execution_identity")
    require(isinstance(identity, list) and len(identity) == 2 and isinstance(identity[0], str)
            and identity[0].startswith(profile["runtime"] + "/device-execution/")
            and len(identity[0].rsplit("/", 1)[-1]) == 64
            and all(x in "0123456789abcdef" for x in identity[0].rsplit("/", 1)[-1])
            and type(identity[1]) is int and identity[1] >= profile["device_execution"]["host_fit"]["minimum_physical_cores"],
            "native opaque execution tuple missing")
    before, after = value.get("adapter_before"), value.get("adapter_after")
    require(isinstance(before, dict) and isinstance(after, dict), "actual adapter observations missing")
    for observed in (before, after):
        uuid = observed.get("uuid")
        require(isinstance(uuid, list) and len(uuid) == 16 and any(uuid)
                and all(type(x) is int and 0 <= x <= 255 for x in uuid), "actual adapter UUID absent")
        require(all(type(observed.get(k)) is int and observed[k] >= 0 for k in
                    ("ordinal", "compute_major", "compute_minor", "driver_api_version",
                     "physical_total_bytes", "total_bytes", "free_bytes", "epoch"))
                and observed["driver_api_version"] >= 12080
                and 0 < observed["total_bytes"] <= observed["physical_total_bytes"]
                and observed["free_bytes"] <= observed["total_bytes"], "actual adapter capacity observation invalid")
        sha = observed.get("driver_image_sha256")
        require(isinstance(sha, str) and len(sha) == 64 and all(x in "0123456789abcdef" for x in sha),
                "driver image proof missing")
    fixed = ("uuid", "ordinal", "compute_major", "compute_minor", "driver_api_version",
             "driver_image_sha256", "physical_total_bytes")
    require(all(before[k] == after[k] for k in fixed), "adapter identity changed during component")


def validate_operations(events, value, component, tokens):
    require(len(events) == 4, "exact load/numeric start-return journal required")
    identity = {"input_sha256": value["request"]["sha256"], "profile": value["profile"],
                "process_id": value["pid"], "native_binary_sha256": value["native_binary_sha256"]}
    previous = 0
    for i, event in enumerate(events):
        kind, ordinal = ("load", 0) if i < 2 else ("numeric", ROWS[tokens][0] + 1)
        require(event.get("schema") == "airk.arbiter.native-operation/1" and event.get("sequence") == i + 1
                and event.get("identity") == identity and event.get("kind") == kind
                and type(event.get("ordinal")) is int and event["ordinal"] == ordinal
                and event.get("phase") == ("started" if i % 2 == 0 else "returned")
                and event.get("clock") == "windows_performance_counter", "native operation identity/order differs")
        component.exact(event.get("operation_input_digest"), None if i < 2 else value["rows"][0]["input_digest"],
                        "operation input differs")
        started, observed, expiry = (event.get(k) for k in
                                     ("started_monotonic_ns", "observed_monotonic_ns", "expires_monotonic_ns"))
        require(all(type(x) is int for x in (started, observed, expiry)) and previous <= observed
                and 0 < started <= observed < expiry == value["original_expires_monotonic_ns"],
                "original operation clock differs")
        if i % 2:
            require(event.get("native_success") is True
                    and component.finite(event.get("native_call_elapsed_us"), 0, 120000000)
                    and started == events[i - 1]["started_monotonic_ns"], "native return proof absent")
        else:
            require(started == observed and event.get("native_success") is None
                    and event.get("native_call_elapsed_us") is None, "start event differs")
        previous = observed


def graph_inputs(plan, base, receipt_pin, component):
    receipt = base.read(base.pinned(receipt_pin))
    require(receipt.get("schema") == "airk.private.owned-cuda-graph-metadata-diagnostic/1"
            and all(receipt.get(key) is True for key in
                    ("complete", "graph_metadata_observed", "diagnostic_only", "whole_phase_deadline_not_refreshed"))
            and all(receipt.get(key) is False for key in
                    ("qualification", "product_admission", "component_pass", "model_pass", "whole_phase_expired"))
            and "failure" in receipt and receipt["failure"] is None
            and receipt.get("post_input_drift") == receipt.get("final_input_errors") == receipt.get("final_proof_errors") == []
            and receipt.get("final_proof_success") == ["native_result", "stdout"]
            and component.finite(receipt.get("elapsed_seconds"), 0) and receipt["elapsed_seconds"] < 120
            and receipt.get("execution_user_sid") == component.SID,
            "successful original owned fresh graph receipt required")
    began, finished, cutoff = (datetime.fromisoformat(receipt[key]) for key in
                               ("started_utc", "finished_utc", "original_deadline"))
    require(all(value.utcoffset() == timedelta() for value in (began, finished, cutoff))
            and began <= finished <= cutoff <= began + timedelta(seconds=120)
            and cutoff <= base.CAMPAIGN,
            "original graph receipt timing differs")
    graph_plan_pin = receipt["plan"]
    graph_plan = base.read(base.pinned(graph_plan_pin))
    require(component.execution_mode(graph_plan) == "graph"
            and graph_plan["limits"] == component.LIMITS and graph_plan["human_sid"] == component.SID
            and Path(graph_plan["graph_provenance_owner"]["path"]) == GRAPH
            and "fresh_lease_owner" in graph_plan and "fresh_bootstrap" in graph_plan,
            "fresh graph plan and original bounds required")
    require(all(key in graph_plan and key in plan and graph_plan[key] == plan[key] for key in GRAPH_LINKS)
            and receipt.get("input_seals") == dict(graph_plan["input_seals"], **{graph_plan_pin["path"]: graph_plan_pin["sha256"]}),
            "graph evidence belongs to another Compact source, model or request")
    graph_root = Path(graph_plan["output_root"])
    require(graph_root.parent == component.ANCHOR and graph_root != Path(plan["output_root"])
            and Path(receipt_pin["path"]) == graph_root / "execution.json", "original graph output location differs")
    require(receipt.get("command") == [plan["native_image"]["path"], "--exact", component.GRAPH_SELECTOR,
                                      "--ignored", "--nocapture", "--test-threads=1"],
            "original graph selector/image command differs")
    custody = receipt.get("graph_custody", {})
    boundary = custody.get("boundary")
    require(custody.get("preparation") == graph_plan["graph_preparation"]
            and custody.get("close_errors") == []
            and type(custody.get("opened_handles")) is int and custody["opened_handles"] > 0
            and type(custody.get("closed_handles")) is int and isinstance(boundary, list)
            and custody["opened_handles"] == custody["closed_handles"] == len(boundary),
            "successful settled graph input custody required")
    native = receipt["native_result"]
    stdout = receipt["stdout"]
    require(Path(native["path"]) == graph_root / "native-result.json"
            and Path(stdout["path"]) == graph_root / "stdout-stderr.log"
            and all(type(pin["bytes"]) is int and 0 < pin["bytes"] <= component.DIAGNOSTIC_PROOFS[key]
                    for key, pin in (("native_result", native), ("stdout", stdout))),
            "complete bounded graph output pins required")
    return receipt, graph_plan, [
        receipt_pin, graph_plan_pin, graph_plan["graph_provenance_owner"], native, stdout,
    ]


def bind_plan(plan, pins, preparing):
    by_path = {row["path"]: row["sha256"] for row in pins}
    require(len(by_path) == len(pins), "duplicate extra Compact evidence path")
    seals = plan["input_seals"]
    for path, sha in by_path.items():
        require(seals.get(path, sha if preparing else None) == sha, "Compact evidence source seal differs: " + path)
    if preparing:
        seals.update(by_path)


def install(component, args, self_pin, contract_pin, before_deadline):
    """Bind only the row-specific seams in a private copy of the exact frozen owner."""
    tokens = args.tokens
    original_input = component.input_facts
    original_preflight = component.preflight

    def input_facts(plan, base):
        native, profile, fixture = original_input(plan, base)
        selected_row(native, fixture, tokens)
        return native, profile, fixture

    preparing = args.prepare

    def preflight(plan, base, **kwargs):
        component.bind_graph_hash_deadline(base, before_deadline)
        if preparing:
            graph_pin = base.pin(args.graph_receipt)
            require(graph_pin["sha256"] == args.graph_receipt_sha256,
                    "reviewed fresh graph receipt differs")
            plan.update(context_owner=self_pin, context_contract=contract_pin, graph_receipt=graph_pin)
        require(plan["context_owner"] == self_pin and plan["context_contract"] == contract_pin
                and plan["selector"] == selector(tokens), "reviewed Compact adapter/case differs")
        graph_pin = plan["graph_receipt"]
        receipt, graph_plan, graph_pins = graph_inputs(plan, base, graph_pin, component)
        pins = [self_pin, contract_pin, *graph_pins]
        bind_plan(plan, pins, preparing)
        settled, owner, native, profile, fixture = original_preflight(plan, base, **kwargs)
        # The graph receipt and actual native observation are from the graph diagnostic.
        # This component has its own independent strict all-CUDA qualification checks.
        owned = receipt["owned"]
        require(settled.settled(owned) and component.finite(owned.get("peak_host_committed_bytes"), 1, 24 << 30)
                and type(owned.get("owned_leader_pid")) is int and owned["owned_leader_pid"] > 0,
                "successful naturally settled graph Job required")
        value = base.read(base.pinned(receipt["native_result"]))
        component.validate_graph_diagnostic(value, graph_plan, owned["owned_leader_pid"], native, profile)
        require(receipt["selected_logs_complete"] is value["selected_logs_complete"],
                "native and retained graph log scope differs")
        return settled, owner, native, profile, fixture

    component.SCHEMA = SCHEMA
    component.SELECTOR = selector(tokens)
    component.input_facts = input_facts
    component.preflight = preflight
    component.validate_native = lambda value, plan, pid, native, profile, fixture: validate_result(
        value, plan, pid, native, profile, fixture, component, tokens
    )
    component.validate_operations = lambda events, value: validate_operations(events, value, component, tokens)


def parse(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument("--prepare", action="store_true")
    modes.add_argument("--execute-reviewed-component", action="store_true")
    parser.add_argument("--tokens", type=int, choices=sorted(ROWS), required=True)
    parser.add_argument("--approved-script-sha256", required=True)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--plan-sha256")
    parser.add_argument("--metadata-receipt", type=Path)
    parser.add_argument("--metadata-receipt-sha256")
    parser.add_argument("--image-packet", type=Path)
    parser.add_argument("--image-packet-sha256")
    parser.add_argument("--powershell", type=Path)
    parser.add_argument("--output-root", type=Path)
    parser.add_argument("--graph-receipt", type=Path)
    parser.add_argument("--graph-receipt-sha256")
    args = parser.parse_args(argv)
    prep = {"metadata_receipt", "metadata_receipt_sha256", "image_packet", "image_packet_sha256",
            "powershell", "output_root", "graph_receipt", "graph_receipt_sha256"}
    expect = prep if args.prepare else {"plan_sha256"}
    if {key for key in prep | {"plan_sha256"} if getattr(args, key) is not None} != expect:
        parser.error("supply exactly the preparation or reviewed execution arguments")
    for key in expect | {"approved_script_sha256", "plan"}:
        value = getattr(args, key)
        ok = value.is_absolute() if isinstance(value, Path) else (
            len(value) == 64 and all(c in "0123456789abcdef" for c in value)
        )
        if not ok:
            parser.error(key + " requires an absolute path or lowercase SHA-256")
    return args


def main(argv=None):
    started = time.monotonic()
    started_utc = datetime.now(timezone.utc)
    args = parse(argv)
    require(os.name == "nt", "reviewed Compact execution requires the same-human Windows host")
    source(SELF, args.approved_script_sha256)
    component = private_module(COMPONENT, source(COMPONENT, COMPONENT_SHA), "fresh_context_component")
    native_data = source(NATIVE, NATIVE_SHA)
    native = private_module(NATIVE, native_data, "fresh_context_native")
    # Clip execution to the outer original window so bootstrap cannot extend it.
    cutoff = min(native.CAMPAIGN, started_utc + timedelta(seconds=120))
    deadline = started + (cutoff - started_utc).total_seconds()
    require(time.monotonic() < deadline, "original Compact parent deadline expired")

    if not args.prepare:
        native.CAMPAIGN = cutoff
    component.bind_graph_hash_deadline(native, deadline)
    primary = native

    def base():
        nonlocal primary
        if primary is not None:
            module, primary = primary, None
            return module
        # The frozen owner retains its separate guarded final receipt write.
        return private_module(NATIVE, native_data, "fresh_context_auxiliary")

    component.base = base
    self_pin = native.pin(SELF)
    require(self_pin["sha256"] == args.approved_script_sha256, "reviewed Compact adapter changed")
    install(component, args, self_pin, native.pin(CONTRACT), deadline)
    result = component.prepare(args) if args.prepare else component.run_loaded(args)
    print(json.dumps(result))


if __name__ == "__main__":
    main()
