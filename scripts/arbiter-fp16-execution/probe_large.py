# SPDX-License-Identifier: Apache-2.0
"""Run the exact Windows Large baseline metadata or disposable placement selector."""
import argparse
import ctypes
import json
import os
import math
import re
from pathlib import Path
import runpy
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
PREFIX = "inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::"
SELECTORS = {"metadata": PREFIX + "fp16_large::emit_cuda_large_fp16_probe_metadata",
             "graph": PREFIX + "fp16_large::cuda_large_fp16_load_only_graph_diagnostic",
             "qkv-metadata": PREFIX + "fp16_large::emit_cuda_large_qkv_fp32_probe_metadata",
             "qkv-graph": PREFIX + "fp16_large::cuda_large_qkv_fp32_load_only_graph_diagnostic",
             "qkv-t32": PREFIX + "bounded_hybrid::cuda_large_qkv_fp32_scientific_t32",
             "t32": PREFIX + "bounded_hybrid::cuda_large_fp16_scientific_t32",
             "projection-t32": PREFIX + "bounded_hybrid::cuda_large_fp16_projection_taps_t32"}
PREPARE = "8e69df4aedb85c556330262051d46c610ab48cec3c2a71aba60039458b6db348"
REFERENCE = "264a3f5a9ddfb9f43a22328c88ce6497624dec87c6c8659fd5b5a7b3a6464ec3"
GRAPH = "c00371ad9407c25bd52999d87327115025b7c6f55a4e585645ca112d858de5bd"
WEIGHTS = "5b053f02795e72ff88123b28f89d3f2c89da055e9ed2228bc60809dd0a2b4270"
MANIFEST = "9433753580c8a370bbea42119aa67ac81306229bcf6876514213f3e2979083b5"
SOURCE = "crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/cuda_qualification/fp16_large.rs"
HOST = 48 << 30


def require(condition, message):
    if not condition:
        raise ValueError(message)


def core(pin):
    return {key: pin[key] for key in ("path", "sha256")}


QKV_STAGE = "0ae64faa17aecc33642c3245b3bf55b6f44a961de50128114ecf5ad8442e295c"
QKV_GRAPH = "1448e83dcd209b120bd22e1dec084092f56adc7e8eec5dcd323e179b800467aa"


def metadata_contract(value, reference, stage=None):
    require(value.get("protected_graph_stage") == (core(stage) if stage else None),
            "Large graph stage differs from the selected recipe")
    require(value["schema"] == "airk.private.large-fp16-probe-metadata/1"
            and value["request"]["sha256"] == PREPARE and value["independent_proof"]["sha256"] == REFERENCE
            and value["reference_input_digest"] == reference["input_digest"]
            and value["graph"] == (QKV_GRAPH if stage else GRAPH) and value["weights"] == WEIGHTS
            and value["model_requests_dispatched"] is value["qualified"] is False,
            "Large metadata lineage, numerical basis or scope differs")


def validate_large_attention(placement):
    expected = {f"/airk_g5/attention/{index}/efficient" for index in range(36)}
    attention = [node for node in placement["nodes"] if node["op"] == "GroupQueryAttention"]
    require(placement["complete"] is True and placement["reported_nodes"] == len(placement["nodes"])
            and len(attention) == 36 and {node["name"] for node in attention} == expected
            and all(node["ep"] == "CUDAExecutionProvider" and node["domain"] == "com.microsoft" and node["subgraph_index"] == 0 for node in attention),
            "complete final placement of all 36 named Large GQA nodes on CUDA is required")


def scoring_result(value, reference, log, shared, *, mode="t32"):
    schema = {"projection-t32":"airk.private.large-fp16-layer0-projection-t32/1",
              "qkv-t32":"airk.private.large-qkv-fp32-scientific-t32/1"}.get(mode, "airk.private.large-fp16-scientific-t32/1")
    require(value["schema"] == schema and value["complete"] is True
            and value["qualified"] is value["product_admission"] is False and value["session_teardown_observed"] is True
            and value["native_host_charge_release_observed"] is value["native_control_cleanup_observed"] is True
            and value["process_cuda_poisoned"] is False and len(value["rows"]) == len(value["profiled_runs"]) == 1,
            "Large scientific scoring or natural settlement evidence differs")
    row, trace = value["rows"][0], value["profiled_runs"][0]
    require(row["tokens"] == trace["tokens"] == 32 and row["input_digest"] == reference["input_digest"]
            and trace["node_count"] == (2486 if mode == "qkv-t32" else 2234) and trace["cpu_nodes"] == 478 and trace["cpu_max_tensor_bytes"] <= 32,
            "Large scientific T32 input or typed ORT profile differs")
    metrics = {}
    for label, reference_key in [("matching_fp16", "matching_fp16"), ("original_fp32", "original_fp32")]:
        metrics[label] = shared["development_metrics"](row["raw_scores"], reference[reference_key])
        for metric in ("raw_error", "mass_error"):
            require(math.isclose(metrics[label][metric], row[label + "_" + metric], rel_tol=0, abs_tol=1e-6),
                    "native and independently computed errors differ")
        require(metrics[label]["argmax_matches"] is row[label + "_argmax_matches"], "Large winner comparison differs")
    require(row["development_screen_pass"] is shared["provisional_development_pass"](metrics["matching_fp16"], metrics["original_fp32"])
            and row["matching_reference_0_001"] is (metrics["matching_fp16"]["raw_error"] <= .001),
            "development or original ceiling verdict differs")
    logs = []
    for raw in log.splitlines():
        if "SdpaKernel=" not in raw:
            continue
        line = raw.removeprefix(f"test {SELECTORS[mode]} ... ").removeprefix("\x1b[36m").removesuffix("\x1b[0m")
        match = re.fullmatch(r"Operator=GroupQueryAttention Node=(\S+) DataType=fp16 SdpaKernel=FLASH_ATTENTION(?: [A-Za-z][A-Za-z0-9]*=\S+)*", line)
        require(match is not None, "Large non-Flash or unrecognized actual attention record")
        logs.append(match.group(1))
    require(len(logs) == 36 and set(logs) == {f"/airk_g5/attention/{i}/efficient" for i in range(36)},
            "actual execution must cover all 36 distinct Large Flash kernels")
    matching, original = metrics["matching_fp16"], metrics["original_fp32"]
    raw_pass = matching["raw_error"] <= .007
    return {"comparisons": metrics, "flash_nodes": logs, "development_screen_pass": row["development_screen_pass"],
            "historical_matching_reference_0_001": row["matching_reference_0_001"],
            "release_matching_raw_limit": .007, "release_matching_raw_pass": raw_pass,
            "t32_matching_comparison_pass": raw_pass and matching["argmax_matches"] and matching["mass_error"] <= .05
                and original["mass_error"] <= .05}


def run(args):
    protected = args.mode in ("qkv-metadata", "qkv-graph", "qkv-t32")
    capture = args.mode == "projection-t32"
    numerical = capture or args.mode in ("t32", "qkv-t32")
    hard_cap = {"qkv-metadata": 16 << 30, "qkv-graph": 22 << 30, "qkv-t32": 22 << 30}.get(args.mode, HOST)
    started = time.monotonic()
    deadline = started + 120
    require(os.name == "nt" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "isolated Windows Python -I -B -S required")
    c = runpy.run_path(str(ROOT / "scripts/cuda-component-execution.py"))
    base = c["base"]()
    shared = runpy.run_path(str(HERE / "probe_compact.py"))
    large = runpy.run_path(str(ROOT / "scripts/cuda-component-execution/large_graph.py"))
    parent, owner = base.helpers({"ownership_helper_root": str(base.MAINTAINED_HELPER_ROOT)})
    require(parent.worker.windows_sid() == base.SID and c["token_owner"](ctypes.c_void_p(-1)) == base.SID, "parent owner differs")
    guard = {"checked_guard": base.pin(ROOT / "scripts/dev/assert-human-execution.ps1"),
             "powershell": shared["exact_pin"](args.powershell, args.powershell_sha256, base)}

    def audit(paths, created=True, *, deadline=deadline):
        c["guard_many"](guard, paths, created, deadline=deadline)

    out = args.output
    require(out.parent == CAMPAIGN and out.resolve(strict=False) == out and not out.exists(), "fresh private campaign output required")
    wrappers = [Path(__file__).resolve(), HERE / "large_placement.py", HERE / "probe_compact.py",
                ROOT / "scripts/cuda-component-execution.py", ROOT / "scripts/cuda-component-execution/large_graph.py",
                ROOT / "scripts/cuda-component-execution/placement_report.py", ROOT / "scripts/cuda-native-execution.py"]
    if capture:
        wrappers.append(HERE / "large_layer0_capture.py")
    audit(wrappers)
    wrapper_pins = [base.pin(file) for file in wrappers]
    inputs = {}
    for key in ("image", "build_receipt", "request", "evidence"):
        path = getattr(args, key)
        audit([path])
        inputs[key] = shared["exact_pin"](path, getattr(args, key + "_sha256"), base)
    image, build, request, evidence = (inputs[k] for k in ("image", "build_receipt", "request", "evidence"))
    manifest = shared["exact_pin"](args.image.with_name("airk-cuda-runtime.json"), MANIFEST, base)
    probe = base.read(request["path"])
    original_runtime = probe["runtime_manifest"]
    if capture or args.mode == "qkv-metadata":
        require(args.image.parent == CAMPAIGN / "windows-cu15-scientific-provider-stage-r1"
                and original_runtime["sha256"] == MANIFEST
                and core(shared["exact_pin"](Path(original_runtime["path"]), MANIFEST, base)) == original_runtime,
                "Large capture requires the original CUDA manifest and its authenticated D-stage copy")
    else:
        require(original_runtime == core(manifest), "Large runtime locator changed")
    require(probe["bounds"]["host_limit_bytes"] == HOST
            and probe["job_limit_bytes"] == HOST and probe["bounds"]["device_limit_bytes"] == 44 << 30
            and probe["bounds"]["deadline_ms"] == 120000
            and (probe["cold_bytes"], probe["warm_bytes"], probe["headroom_bytes"]) == (40 << 30, 8 << 30, 4 << 30),
            "Large request process/Job, adapter, arena or expiry differs")
    snapshot = args.build_receipt.parent / "retained-native-sources" if numerical or protected else None
    cargo_chain = None
    if args.mode == "qkv-graph":
        direct = CAMPAIGN / "windows-large-protected-qkv-metadata-r1/execution.json"
        audit([direct])
        pin = shared["exact_pin"](direct, "a91f80a6a569c9db1f7cb07063e9778796de833e34da01b4d0e3ec7550b9aa46", base)
        previous = base.read(pin["path"])
        custody = previous["owned"]["suspended_assignment"]
        require(previous["mode"] == "qkv-metadata" and previous["complete"] is True
                and previous["failure"] is None and previous["whole_phase_expired"] is False
                and parent.settled(previous["owned"]) and previous["inputs"]["build_receipt"] == build
                and previous["inputs"]["image"] == image and previous["runtime_manifest"] == manifest
                and previous["native"] == evidence and custody["image"] == core(image)
                and custody["execution_user_sid"] == custody["default_owner_sid"] == base.SID,
                "protected graph requires its directly authenticated original Cargo image and metadata")
        inputs["direct_cargo_proof"] = pin
        cargo_chain = {"build":build, "build_log":previous["build_log"], "linked_image":previous["linked"],
                       "compile_time_cuda_approvals":previous["build_approvals"]}
    log_pin, linked, sources = shared["linked_image"](build, image, base, audit, "metadata",
                                                      source_snapshot=snapshot, cargo_chain=cargo_chain)
    native_sources = base.read(build["path"], 32 << 20)["groups"]["native"]["files"]

    def source_pin(relative):
        sha = native_sources[relative]
        path = ROOT / relative if snapshot is None else snapshot / (sha + ".source")
        audit([path])
        return shared["exact_pin"](path, sha, base)

    sources[SOURCE] = source_pin(SOURCE)
    if numerical:
        for relative in (SOURCE.replace("fp16_large.rs", "bounded_hybrid.rs"), SOURCE.replace("fp16_large.rs", "hybrid_profile_trace.rs"),
                         "crates/airk-retrieval/src/arbiter/runtime/bounded_hybrid.rs"):
            sources[relative] = source_pin(relative)
    approvals = shared["decision_build_approvals"](log_pin, linked, manifest, base, audit, cargo_chain=cargo_chain)
    if capture:
        for leaf in ("bounded_hybrid/layer0.rs", "cuda/run.rs"):
            relative = "crates/airk-retrieval/src/arbiter/runtime/" + leaf
            sources[relative] = source_pin(relative)
        capture_owner = runpy.run_path(str(HERE / "large_layer0_capture.py"))
        preflight, intended = capture_owner["read"](CAMPAIGN / "large-original-layer0-t32-capture-preflight-r1.json",
            "25787597728bdc24f09e2140082d0e45425b73094afe32780c69e26340d0eedd")
        inputs["capture_preflight"] = preflight
        baseline, original_native = capture_owner["read"](Path(intended["original_native"]["path"]), capture_owner["NATIVE"])
        inputs["original_native"] = baseline
    stage = None
    if protected:
        stage_file = CAMPAIGN / "large-fp32-qkv-stage-r1/stage.json"
        audit([stage_file])
        stage = shared["exact_pin"](stage_file, QKV_STAGE, base)
        inputs["protected_stage"] = stage
        for relative in (SOURCE.replace("/cuda_qualification/fp16_large.rs", "/cuda_qualification.rs"),
                         "crates/airk-retrieval/src/arbiter/runtime/bounded_hybrid/layer0.rs",
                         "crates/airk-retrieval/src/arbiter/runtime/cuda/diagnostic.rs"):
            sources[relative] = source_pin(relative)
    metadata = args.mode in ("metadata", "qkv-metadata")
    evidence_value = base.read(evidence["path"])
    if metadata:
        require(request["sha256"] == PREPARE and evidence["sha256"] == REFERENCE and probe["kind"] == "prepare",
                "exact Large CPU lineage and independent T32 required")
        reference = evidence_value
    else:
        require(probe["kind"] == "probe" and evidence_value["probe_request"] == core(request)
                and probe["gpu_input"] == evidence_value["native_input"], "metadata did not issue this GPU request")
        ref = evidence_value["independent_proof"]
        audit([Path(ref["path"])])
        shared["exact_pin"](Path(ref["path"]), REFERENCE, base)
        reference = base.read(ref["path"])
        metadata_contract(evidence_value, reference, stage)
    require(reference["graph"]["sha256"] == GRAPH and reference["weights"]["sha256"] == WEIGHTS
            and reference["verification"]["graph"]["layers"] == 36 and reference["forward_count"] == 1,
            "Large independent reference geometry differs")
    if args.mode == "qkv-t32":
        path = CAMPAIGN / "large-fp32-qkv-independent-t32-r1/reference.json"
        audit([path])
        matching = shared["exact_pin"](path, "bc91621b62faeec0963caec75321c34b76e36f4bd66bbb7df6746d2872ab6191", base)
        specific = base.read(matching["path"])
        require(specific["graph"]["sha256"] == QKV_GRAPH and specific["weights"] == reference["weights"]
                and specific["input_digest"] == reference["input_digest"] and specific["original_fp32"] == reference["original_fp32"]
                and specific["candidate"]["protected_routes_in_model_order"] == 108
                and specific["forward_count"] == 1 and specific["device"] == "cpu",
                "protected Large scientific reference lineage differs")
        inputs["matching_reference"] = matching
        ref, reference = core(matching), specific
    if numerical:
        source_inputs = reference["source_inputs"]
        audit([Path(source_inputs["path"])])
        base.pinned(source_inputs)
        candidates = base.read(source_inputs["path"])["rows"]
        selected = [row for row in candidates if row["input"]["input_digest"] == reference["input_digest"]]
        require(len(selected) == 1 and len(selected[0]["input"]["input_ids"]) == 32
                and selected[0]["reference_original_fp32"] == reference["original_fp32"], "Large reference input pairing differs")
        graph_sha = ("0d00e2fabdb5fe54474041fb1f2787502349b78ce40436b955cd4433633a4462" if protected
                     else "fff6dbe2f0c2fac4b9ba215da7d5029421d2706ee48c01c2485b9f157ee15ca6")
        graph = shared["exact_pin"](args.graph, graph_sha, base)
        inputs["reviewed_graph"] = graph
        audit([args.graph])
        graph_value = base.read(graph["path"])
        if protected:
            require(graph_value["optimizer_profile"] == "protected_qkv_fp32", "protected optimizer differs")
            shared["validate_projection_graph"](graph_value["graph_metadata"], large_qkv=True)
        controls = runpy.run_path(str(HERE / "large_placement.py"))["cpu_controls"](graph_value["graph_metadata"])
        require(controls["maximum_vector_elements"] == 4 and controls["maximum_value_bytes"] == 32
                and controls["summed_unique_payload_bytes"] == 12067 and controls["unique_cpu_values"] == 670,
                "reviewed Large graph control bounds differ")
        require(graph_value["request"] == core(request) and graph_value["profile"] == evidence_value["profile"]
                and graph_value["diagnostic_complete"] is True, "reviewed graph bound to a different Large candidate")
    cmd = [image["path"], "--exact", SELECTORS[args.mode], "--ignored", "--nocapture", "--test-threads=1"]
    result = {"schema": "airk.private.owned-large-fp16-native/1", "mode": args.mode, "complete": False,
              "qualification": False, "failure": None, "command": cmd, "inputs": inputs, "runtime_manifest": manifest,
              "linked": linked, "build_log": log_pin, "build_approvals": approvals, "sources": sources, "wrappers": wrapper_pins,
              "original_deadline": deadline, "requested_host_reservation_bytes": HOST, "actual_hard_cap_bytes": hard_cap,
              "host_before_job": large["host_preflight"](require)}
    c["mkdir"](guard, out, deadline=deadline)
    temp, stdout, native, directory, receipt = (out / leaf for leaf in ("temp", "stdout.log", "native.json", "metadata", "execution.json"))
    try:
        c["mkdir"](guard, temp, deadline=deadline)
        journal = native.with_suffix(".operations.jsonl")
        prefix = out / "ort-profile"
        audit([stdout, native, directory, receipt, journal, prefix], False)
        env = {k: v for k, v in os.environ.items() if not k.upper().startswith(("AIRK_", "ORT_", "PYTHON", "CUDA_", "CUDNN_"))}
        env.update(PATH=r"C:\Windows\System32;C:\Windows", TEMP=str(temp), TMP=str(temp), TMPDIR=str(temp),
                   AIRK_PRIVATE_CENSUS_BASE=str(temp), CUDA_CACHE_PATH=str(temp),
                   AIRK_CUDA_QUALIFICATION_INPUT=request["path"], AIRK_CUDA_QUALIFICATION_INPUT_SHA256=request["sha256"],
                   AIRK_CUDA_QUALIFICATION_OUTPUT=str(native))
        if stage:
            env.update(AIRK_LARGE_QKV_STAGE=stage["path"], AIRK_LARGE_QKV_STAGE_SHA256=stage["sha256"])
        if metadata:
            env.update(AIRK_CUDA_METADATA_DIRECTORY=str(directory), AIRK_CUDA_FP16_INDEPENDENT_PROOF=evidence["path"],
                       AIRK_CUDA_FP16_INDEPENDENT_PROOF_SHA256=evidence["sha256"])
        else:
            env.update(ORT_CUDA_GEMM_OPTIONS="2", ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO="1")

        if numerical:
            env.update(AIRK_CUDA_HYBRID_GRAPH=graph["path"], AIRK_CUDA_HYBRID_GRAPH_SHA256=graph["sha256"],
                       AIRK_CUDA_HYBRID_ORT_PROFILE_PREFIX=str(prefix), AIRK_CUDA_FP16_INDEPENDENT_PROOF=ref["path"],
                       AIRK_CUDA_FP16_INDEPENDENT_PROOF_SHA256=ref["sha256"])

        def suspended(api, job, info, child_deadline):
            proof = parent.suspended_authentication(api, info, deadline, child_deadline, base.SID, image, owner.image_proof)
            require(c["token_owner"](info.process) == base.SID and owner.job_snapshot(api, job) == ([int(info.pid)], 1, 1),
                    "suspended Large child owner or Job membership differs")
            return dict(proof, default_owner_sid=base.SID)

        with open(os.devnull, "rb") as stdin, stdout.open("xb") as log:
            result["owned"] = owner.run_windows_owned_training(cmd, ROOT, log, deadline - time.monotonic(), env,
                stdin, hard_cap, before_resume=suspended, absolute_deadline=deadline)
        owned = result["owned"]
        require(parent.settled(owned) and 0 < owned["peak_host_committed_bytes"] <= hard_cap, "Large native child failed or remained active")
        audit(list(out.rglob("*")))
        require(not any(temp.iterdir()) and 0 < stdout.stat().st_size <= 64 << 20
                and "1 passed; 0 failed" in stdout.read_text(encoding="utf-8"), "one successful selector required")
        value = base.read(str(native), c["DIAGNOSTIC_PROOFS"]["native_result"])
        require(value["pid"] == owned["owned_leader_pid"] and value["native_binary_sha256"] == image["sha256"]
                and value["request"] == core(request) and value["runtime_manifest"] == original_runtime
                and (not capture or value["installed_runtime_manifest"] == core(manifest))
                and value["execution_user"] == base.SID and value["actual_job"]["process_memory_limit_bytes"] == hard_cap
                and value["actual_job"]["job_memory_limit_bytes"] == hard_cap,
                "native Large CPU, process/Job, image or manifest differs")
        if metadata:
            metadata_contract(value, reference, stage)
            if stage:
                require(value["probe_runtime_manifest"] == core(manifest), "protected graph runtime locator differs")
            for key in ("probe_request", "native_input", "profile_pin", "source_manifest", "calibration"):
                file = Path(value[key]["path"])
                require(file.parent == directory, "metadata output escaped planned directory")
                shared["exact_pin"](file, value[key]["sha256"], base)
            require(len(list(directory.iterdir())) == 5, "Large metadata roster differs")
        elif numerical:
            require(value["reviewed_graph"] == core(graph) and value["independent_proof"] == ref
                    and value["reference_inputs"]["sha256"] == reference["source_inputs"]["sha256"]
                    and value["profile"] == evidence_value["profile"], "Large numerical evidence is detached from the reference")
            c["validate_operations"](shared["read_operations"](journal), value)
            profile = value["runtime_profile"]
            require(Path(profile["path"]).parent == out and Path(profile["path"]).name.startswith(prefix.name),
                    "Large ORT profile escaped the owned prefix")
            base.pinned(profile)
            result.update(scoring_result(value, reference, stdout.read_text(encoding="utf-8"), shared, mode=args.mode))
            if capture:
                result["capture"] = capture_owner["check_capture"](value, original_native)
            result.update(journal=base.pin(journal), runtime_profile=profile)
        else:
            require(value["diagnostic_complete"] is True and value["outcome"] == "graph_metadata_observed"
                    and value["forward_dispatched"] is value["model_requests_dispatched"] is value["qualification"] is False
                    and value["product_admission"] is False and value["profile"] == evidence_value["profile"]
                    and value["source_manifest"] == evidence_value["source_manifest"] and value["native_input"] == probe["gpu_input"]
                    and value["native_host_charge_release_observed"] is value["native_control_cleanup_observed"] is True
                    and value["process_cuda_poisoned"] is False, "Large load-only or settlement evidence differs")
            c["validate_graph_metadata"](value["graph_metadata"], value["placement"])
            validate_large_attention(value["placement"])
            report = runpy.run_path(str(ROOT / "scripts/cuda-component-execution/placement_report.py"))
            result["graph_summary"] = report["summarize"](value["graph_metadata"])
            result["cpu_controls"] = runpy.run_path(str(HERE / "large_placement.py"))["cpu_controls"](value["graph_metadata"])
        if args.mode == "qkv-graph":
            require(value["optimizer_profile"] == "protected_qkv_fp32", "protected session optimizer differs")
            result["protected_routes"] = shared["validate_projection_graph"](value["graph_metadata"], large_qkv=True)
        for row in (*inputs.values(), manifest, *sources.values(), *wrapper_pins):
            base.pinned(row)
        require(time.monotonic() < deadline, "Large owned diagnostic exceeded original deadline")
        result.update(complete=True, native=base.pin(native), stdout=base.pin(stdout))
    except BaseException as exc:
        result["failure"] = f"{type(exc).__name__}: {exc}"
        if hasattr(exc, "owned_cleanup"):
            result["owned_cleanup"] = exc.owned_cleanup
    finally:
        parent.final_phase_observation(result, deadline, started)
        outputs = list(out.rglob("*"))
        if outputs:
            audit(outputs, deadline=None)
        c["write_new"](guard, receipt, result)
    return result["complete"]


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=SELECTORS)
    for argument in ("request", "evidence", "image", "build-receipt", "powershell"):
        parser.add_argument("--" + argument, type=Path, required=True)
        parser.add_argument("--" + argument + "-sha256", required=True)
    parser.add_argument("--graph", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    sys.exit(0 if run(parser.parse_args()) else 1)
