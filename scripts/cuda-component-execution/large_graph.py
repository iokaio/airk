# SPDX-License-Identifier: Apache-2.0
"""Run the owned Windows Large FP32 graph or four-width profiling experiment."""

import argparse
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


def scoring_inputs(paths, base, guard, require):
    guard(paths, True)
    pins = {}
    for (label, (_variable, digest)), path in zip(SCORING_INPUTS.items(), paths):
        pin = base.pin(path)
        require(pin["sha256"] == digest, "reviewed Large input differs: " + label)
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


def validate_scoring(native, owned, pins, rows, journal, out, base, guard, require):
    require(native["schema"] == "airk.private.cuda-large-bounded-hybrid-profile/1"
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
    require(len(native["rows"]) == 4
            and [run["tokens"] for run in native["profiled_runs"]] == [32, 128, 1024, 8192]
            and all(run["node_count"] == 3570 and run["cpu_nodes"] == 1663
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
                    ("same_artifact_raw_error", .001), ("original_fp32_raw_error", .001),
                    ("same_artifact_mass_error", .05), ("original_fp32_mass_error", .05))),
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
    args = parser.parse_args()
    score_paths = (args.graph, args.retained, args.supplemental)
    if any(score_paths) and not all(score_paths):
        parser.error("scoring requires --graph, --retained and --supplemental together")
    scoring = all(score_paths)
    candidate = args.candidate_metadata is not None
    if candidate != (args.candidate_metadata_sha256 is not None):
        parser.error("batch-view mode requires --candidate-metadata and --candidate-metadata-sha256 together")
    if candidate and scoring:
        parser.error("batch-view graph observation and scoring cannot be combined")

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
    if candidate:
        metadata, input_pins = candidate_inputs(
            args.candidate_metadata, args.candidate_metadata_sha256, request, probe, base,
            lambda paths, created: guard(base_guard, paths, created), require,
        )
    elif scoring:
        input_pins, rows, journal = scoring_inputs(
            score_paths, base, lambda paths, created: guard(base_guard, paths, created), require,
        )
    else:
        input_pins = {}

    start = time.monotonic()
    started_utc = datetime.datetime.now(datetime.timezone.utc)
    deadline = start + 120.0
    mkdir(base_guard, args.output, deadline=deadline)
    temp = args.output / "temp"
    mkdir(base_guard, temp, deadline=deadline)
    stdin_path = args.output / "stdin.txt"
    stdout_path = args.output / "stdout-stderr.log"
    native_path = args.output / "native-result.json"
    receipt_path = args.output / "execution.json"
    planned_outputs = [stdin_path, stdout_path, native_path, receipt_path]
    if scoring:
        planned_outputs.extend((args.output / "native-result.operations.jsonl", args.output / "ort-profile"))
    guard(base_guard, planned_outputs, deadline=deadline)

    selector = SCORING_SELECTOR if scoring else SELECTOR
    command = [str(args.image), "--ignored", "--exact", selector, "--test-threads=1"]
    receipt = {
        "schema": ("airk.private.owned-large-cuda-scoring/1" if scoring
                   else "airk.private.owned-large-cuda-graph-observation/1"),
        "complete": False, "qualification": False,
        "model_requests_dispatched": None if scoring else False,
        "started_utc": started_utc.isoformat(), "wall_seconds": 120, "host_bytes": HOST_BYTES,
        "command": command, "request": request, "image": image, "build": build, "build_log": log_pin,
        "execution_user_sid": base.SID, "failure": None,
    }
    if candidate:
        receipt["candidate_inputs"] = input_pins
        receipt["runtime_manifest"] = manifest_pin
    elif scoring:
        receipt["scoring_inputs"] = input_pins
        receipt["runtime_manifest"] = manifest_pin

    def authenticate(api, job, info, child_deadline):
        proof = parent.suspended_authentication(api, info, deadline, child_deadline, base.SID,
                                                image, owner.image_proof)
        require(component["token_owner"](info.process) == base.SID,
                "suspended native default owner differs")
        require(owner.job_snapshot(api, job) == ([int(info.pid)], 1, 1),
                "the suspended native image must be the only Job member")
        proof["default_owner_sid"] = base.SID
        return proof

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
            for label, (variable, digest) in SCORING_INPUTS.items():
                env[f"AIRK_CUDA_HYBRID_{variable}"] = input_pins[label]["path"]
                env[f"AIRK_CUDA_HYBRID_{variable}_SHA256"] = digest
            env["AIRK_CUDA_HYBRID_ORT_PROFILE_PREFIX"] = str(args.output / "ort-profile")
        with stdin_path.open("xb"):
            pass
        with stdin_path.open("rb") as stdin, stdout_path.open("xb") as stdout:
            guard(base_guard, [stdin_path, stdout_path], True, deadline=deadline)
            owned = owner.run_windows_owned_training(
                command, ROOT, stdout, deadline - time.monotonic(), env, stdin, HOST_BYTES,
                before_resume=authenticate, absolute_deadline=deadline,
            )
        receipt["owned"] = owned
        outputs = list(args.output.rglob("*"))
        guard(base_guard, outputs, True, deadline=deadline)
        receipt["stdout"] = base.pin(stdout_path)
        if native_path.exists():
            receipt["native_result"] = base.pin(native_path)
        if scoring:
            journal_path = args.output / "native-result.operations.jsonl"
            if journal_path.exists():
                receipt["operations"] = base.pin(journal_path)
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
            require(native["request"] == {"path": request["path"], "sha256": REQUEST_SHA}
                    and native["native_input"] == probe["gpu_input"]
                    and native["runtime_manifest"] == probe["runtime_manifest"]
                    and native["bounds"] == probe["bounds"],
                    "native Large scoring did not consume the exact request")
            trace, operations = validate_scoring(
                native, owned, input_pins, rows, journal, args.output, base,
                lambda paths, created: guard(base_guard, paths, created, deadline=deadline), require,
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
                    *input_pins.values(), *(pin for pin in outputs if pin)):
            base.pinned(pin)
        require(time.monotonic() < deadline, "original Large deadline expired")
        receipt["complete"] = True
    except BaseException as error:
        receipt["failure"] = f"{type(error).__name__}: {error}"
        for key in ("emergency_cleanup", "owned_cleanup"):
            cleanup = getattr(error, key, None)
            if cleanup is not None:
                receipt[key] = cleanup
        raise
    finally:
        receipt["elapsed_seconds"] = time.monotonic() - start
        with receipt_path.open("x", encoding="utf-8") as stream:
            json.dump(receipt, stream, ensure_ascii=False, indent=2)
            stream.write("\n")
        guard(base_guard, [receipt_path], True)


if __name__ == "__main__":
    main()
