# SPDX-License-Identifier: Apache-2.0
"""Produce an owned independent CPU T32 reference for the staged Compact Q/K/V variant."""

import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import sys
import time


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
STAGE = CAMPAIGN / "compact-fp16-qkv-fp32-staged-r1/stage.json"
STAGE_SHA = "87ec206416eda607ff1585809d085e37fc4f8b99a5fc029308b5c78218cefcc2"
BASE_ARGUMENTS = {
    "conversion_plan": CAMPAIGN / "compact-fp16-present-conversion-plan-r2/compact.json",
    "conversion_plan_sha256": "b43ddbceb00a82b9ae5cc700bb3e6b633d1ed966cc028592c6e0ab5fe836e715",
    "conversion_receipt": CAMPAIGN / "compact-fp16-present-conversion-owned-r2/execution.json",
    "conversion_receipt_sha256": "547777e60cd994968849c08b5dc158432097ece6b09da37e5008fa7c34b1cc56",
}
BASE_VERIFIER_SHA = "50d5c7237cd057a1d08ea79794bbdda0f1e141438a2d22580a760e418f9e3d75"
GRAPH_VERIFIER_SHA = "160facf30e6bb1b0d5881d26a7c705c3c66ad160f74f0014d7ae5647e6927cf9"
SCHEMA = "airk.private.compact-fp32-qkv-t32-reference/1"
OWNED_SCHEMA = "airk.private.owned-compact-fp32-qkv-t32-reference/1"
MARKER = b"AIRK_COMPACT_FP32_QKV_T32_REFERENCE "
MAX_LOG = 64 << 20
MAX_PROOF = 16 << 20
ROUTES = tuple(f"layers.{layer}.self_attn.{projection}_proj.weight"
               for layer in range(28) for projection in ("q", "k", "v"))


def require(condition, message):
    if not condition:
        raise ValueError(message)


def load_base():
    path = HERE / "verify_compact.py"
    raw = path.read_bytes()
    digest = hashlib.sha256(raw).hexdigest()
    require(digest == BASE_VERIFIER_SHA, "unchanged maintained Compact verifier required")
    spec = importlib.util.spec_from_file_location("compact_projection_base_verifier", path)
    legacy = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = legacy
    spec.loader.exec_module(legacy)
    return legacy, {"path": str(path), "bytes": len(raw), "sha256": digest}


def read_stage(legacy, conversion, plan_pin, token, check):
    stage, stage_pin = legacy.read_pinned_json(STAGE, STAGE_SHA, check)
    conversion.windows_owner(STAGE)
    require(stage["schema"] == "airk.private.compact-fp32-qkv-projection-candidate/1"
            and stage["complete"] is True and stage["qualification"] is stage["product_admission"] is False
            and stage["native_load_attempts"] == stage["native_forward_attempts"] == 0
            and stage["conversion_plan"] == plan_pin and stage["execution_token"] == token
            and stage["graph_proof"] == {"projection_count": 84, "added_cast_count": 252,
                                         "baseline_node_count": 4538, "candidate_node_count": 4790},
            "reviewed staged projection receipt differs")
    candidate = {key: stage["candidate_" + key] for key in ("graph", "weights")}
    require(candidate["graph"]["path"] == str(STAGE.with_name("model.onnx"))
            and candidate["weights"]["path"] == str(STAGE.with_name("weights.bin"))
            and candidate["graph"]["sha256"] != stage["baseline_graph"]["sha256"]
            and all(candidate["weights"][key] == stage["baseline_weights"][key] for key in ("sha256", "bytes")),
            "staged graph or immutable FP16 storage differs")
    source = next((pin for pin in stage["projection_sources"]
                   if pin["path"] == str(HERE / "verify_projection_candidate_graph.py")), None)
    require(source is not None and source["sha256"] == GRAPH_VERIFIER_SHA,
            "exact independent projection graph verifier required")
    return stage, stage_pin, candidate, source


def projection_scores(independent, row, check):
    torch = independent.torch
    require((independent.layers, independent.hidden, independent.qheads, independent.kvheads, independent.dim)
            == (28, 2048, 16, 8, 128)
            and torch.device(independent.device).type == "cpu"
            and all(name in independent.weights for name in ROUTES), "exact Compact FP16 publisher geometry required")
    torch.set_float32_matmul_precision("highest")
    expected = set(ROUTES)
    observed = []
    original_linear = independent._linear_half

    def linear_half(value, name):
        if name not in expected:
            return original_linear(value, name)
        check()
        weight = independent.weights[name]
        require(value.dtype == weight.dtype == torch.float16 and value.device.type == weight.device.type == "cpu",
                "projection variant requires independent CPU half operands")
        output = torch.nn.functional.linear(value.float(), weight.float()).half()
        require(torch.isfinite(output).all().item(), "nonfinite independently projected half output")
        observed.append(name)
        return output

    independent._linear_half = linear_half
    try:
        scores = independent.forward(row, context_flash_single_tile=True)
    finally:
        del independent._linear_half
    require(tuple(observed) == ROUTES, "exact 84 independent projection calls in model order required")
    require(scores.dtype == torch.float32 and scores.device.type == "cpu" and tuple(scores.shape) == (1, 8)
            and not torch.cuda.is_initialized(), "CPU-only FP32-ABI Compact reference vector required")
    return scores[0].tolist()


def worker(deadline, expected_source):
    started = time.monotonic()

    def check():
        if time.monotonic() >= deadline:
            raise TimeoutError("original Compact projection reference deadline expired")

    legacy, legacy_pin = load_base()
    require(0 < deadline - started <= legacy.MAX_WALL_SECONDS and os.environ.get("CUDA_VISIBLE_DEVICES") == "",
            "original bounded CPU-only projection reference required")
    cache = Path(os.environ.get("TORCHINDUCTOR_CACHE_DIR", ""))
    require(cache.is_absolute() and cache.is_relative_to(CAMPAIGN) and cache.resolve(strict=True) == cache
            and cache.is_dir() and not cache.is_symlink() and not cache.is_junction(),
            "existing audited Torch cache directory required")
    sys.addaudithook(lambda event, values: legacy.read_only(event, values, cache))
    conversion, plan, plan_pin, prior_pin, proofs, token = legacy.authenticated_conversion(
        argparse.Namespace(**BASE_ARGUMENTS), check)
    conversion.windows_owner(cache)
    source = conversion.pin(Path(__file__).resolve(), check)
    require(source["sha256"] == expected_source and conversion.pin(sys.executable, check) == plan["python"]
            and sys.pycache_prefix == plan["cache_prefix"] and not Path(plan["cache_prefix"]).exists(),
            "actual reviewed worker, interpreter or absent bytecode prefix differs")
    stage, stage_pin, candidate, graph_source = read_stage(legacy, conversion, plan_pin, token, check)
    baseline = {key: proofs[key] for key in ("graph", "weights")}
    require(baseline == {key: stage["baseline_" + key] for key in baseline},
            "stage does not bind the original authenticated conversion")
    for pin in (*candidate.values(), graph_source):
        conversion.windows_owner(Path(pin["path"]))
        conversion.pinned(pin, check)

    original, selected = conversion.source_facts(plan["source_plan"], "compact", check)
    require(plan["source_graph"] == selected["original_graph"] and plan["source_graph"]["sha256"] == legacy.ORIGINAL_GRAPH
            and plan["source_closure"] == selected["source_closure"] and plan["python"] == original["python"]
            and plan["expected_weights_bytes"] == candidate["weights"]["bytes"] == selected["expected_weights_bin_bytes"]
            and plan["expected_attention_count"] == len(selected["expected_attention_nodes"]) == 28,
            "original Compact source identity differs")
    original_graph = Path(plan["source_graph"]["path"])
    original_files, closure_pin = legacy.read_pinned_json(plan["source_closure"]["path"], plan["source_closure"]["sha256"], check)
    require(closure_pin == plan["source_closure"], "original Compact closure length differs")
    reference, publisher_files = legacy.exact_reference(check)
    publisher_root = legacy.PUBLISHER_ROOT / "candidate-assets/Qwen--Qwen3-1.7B"
    for name, pin in publisher_files.items():
        require(Path(name).name == name and pin["path"] == str(publisher_root / name), "publisher source escaped its path")
        conversion.pinned(pin, check)

    reference_sources = [source, legacy_pin, graph_source]
    for relative, digest in legacy.REFERENCES.items():
        pin = conversion.pin(ROOT / relative, check)
        require(pin["sha256"] == digest, "independent Compact reference source changed")
        reference_sources.append(pin)
    runtime, runtime_pin = legacy.read_pinned_json(
        plan["runtime_manifest"]["path"], plan["runtime_manifest"]["sha256"], check)
    import_pins = {row["path"]: row for row in plan["input_files"]}
    python_home = Path(plan["python"]["path"]).parent
    require(runtime_pin == plan["runtime_manifest"] and import_pins.get(runtime_pin["path"]) == runtime_pin
            and runtime["roots"] == runtime["import_paths"]
                == [str(python_home / "Lib"), str(python_home / "DLLs"), original["python_site"]]
            and all(path == row["path"] and import_pins.get(path) == row for path, row in runtime["files"].items()),
            "reviewed projection reference runtime closure differs")
    sys.path.extend(runtime["import_paths"])
    import onnx
    import torch

    half = legacy.load_module(ROOT / legacy.REFERENCE_OWNER, "compact_projection_independent_half")
    graph_verifier = legacy.load_module(Path(graph_source["path"]), "compact_projection_graph_verifier")
    import_pins.update({pin["path"]: pin for pin in reference_sources})
    legacy.loaded_runtime(import_pins, conversion.pin, check)
    graph = Path(baseline["graph"]["path"])
    typed_graph = graph_verifier.verify_projection_candidate_graph(
        onnx.load_model(graph, load_external_data=False),
        onnx.load_model(candidate["graph"]["path"], load_external_data=False), onnx)
    require(typed_graph == stage["graph_proof"], "actual staged graph differs from reviewed projection proof")

    torch.set_num_threads(2)
    torch.set_num_interop_threads(1)
    independent, publisher = half.from_authenticated_original(
        torch, "compact", ROOT / legacy.PUBLISHER_OWNER, legacy.PUBLISHER_ROOT, "cpu")
    weights = Path(baseline["weights"]["path"])
    baseline_verification = half.verify_actual_half_parameters(
        independent, graph, onnx, {weights.name: weights}, candidate_sha256=baseline["graph"]["sha256"],
        original_graph=original_graph, original_files=original_files)
    check()
    reference["matching_qkv_fp32"] = projection_scores(independent, reference["input"], check)
    require(legacy.finite_scores(reference["matching_qkv_fp32"]), "eight finite independent Compact scores required")
    for pin in (plan_pin, prior_pin, stage_pin, plan["source_graph"], closure_pin, runtime_pin,
                *baseline.values(), *candidate.values(), *publisher_files.values(), *reference_sources,
                reference["fixture"], reference["producer"]):
        conversion.pinned(pin, check)
    for name, pin in original_files.items():
        conversion.pinned(dict(pin, path=str(original_graph.parent / name)), check)
    loaded = legacy.loaded_runtime(import_pins, conversion.pin, check)
    return {
        "schema": SCHEMA, "complete": True, "model": legacy.MODEL,
        "candidate": candidate, "baseline_candidate": baseline, "stage_receipt": stage_pin,
        "conversion_plan": plan_pin, "conversion_receipt": prior_pin,
        "original_graph": plan["source_graph"], "original_closure": closure_pin,
        "publisher": publisher, "publisher_files": publisher_files,
        "verification": {"baseline": baseline_verification, "projection_graph": typed_graph},
        "reference": reference, "reference_sources": reference_sources, "loaded_runtime": loaded,
        "recipe": {"qkv_fp32_routes": list(ROUTES), "context_flash_single_tile": True},
        "worker": source, "pid": os.getpid(), "execution_token": token,
        "original_deadline_monotonic": deadline, "reference_device": "cpu",
        "model_inference": True, "torch_forward_invocations": 1,
        "native_execution": False, "qualification": False, "product_admission": False,
    }


def run_owned(out):
    started = time.monotonic()
    legacy, legacy_pin = load_base()
    deadline = started + legacy.MAX_WALL_SECONDS

    def check():
        if time.monotonic() >= deadline:
            raise TimeoutError("original Compact projection reference deadline expired")

    plan, plan_pin = legacy.read_pinned_json(BASE_ARGUMENTS["conversion_plan"], BASE_ARGUMENTS["conversion_plan_sha256"], check)
    prior, prior_pin = legacy.read_pinned_json(
        BASE_ARGUMENTS["conversion_receipt"], BASE_ARGUMENTS["conversion_receipt_sha256"], check)
    wrapper = ROOT / "scripts/arbiter-fp16-execution.py"
    raw = wrapper.read_bytes()
    require(plan["wrapper"] == {"path": str(wrapper), "bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()},
            "reviewed conversion wrapper differs")
    conversion = legacy.load_module(wrapper, "compact_projection_conversion_parent")
    token = conversion.windows_owner()
    require(plan["schema"] == conversion.SCHEMA and plan["model"] == "compact"
            and plan["human_sid"] == conversion.SID and plan["repository_root"] == str(ROOT)
            and prior["complete"] is True and prior["plan"] == plan_pin
            and conversion.pin(sys.executable, check) == plan["python"], "reviewed conversion and interpreter required")
    stage, stage_pin, candidate, graph_source = read_stage(legacy, conversion, plan_pin, token, check)
    helper_root = ROOT / "scripts/cuda-native-execution/helpers"
    for leaf, digest in conversion.HELPERS.items():
        pin = plan["helpers"][leaf]
        require(pin["path"] == str(helper_root / leaf) and pin["sha256"] == digest, "reviewed owned helper differs")
        conversion.pinned(pin, check)
    sys.path.insert(0, str(helper_root))
    parent = legacy.load_module(helper_root / "reference_parent_no_site_r2.py", "compact_projection_parent")
    owner = legacy.load_module(helper_root / "reference_owned.py", "compact_projection_job")
    require(Path(parent.worker.__file__) == helper_root / "reference_worker.py", "owned reference dependency shadowed")
    require(out.is_absolute() and out.parent == CAMPAIGN and out.resolve(strict=False) == out and not out.exists(),
            "fresh checked direct campaign child required")
    temporary = out / "temp"
    for directory in (out, temporary):
        conversion.guard(plan, directory, deadline=deadline)
        directory.mkdir()
        conversion.guard(plan, directory, True, deadline=deadline)
    log, proof, receipt_path = (out / name for name in ("stdout-stderr.log", "compact-qkv-fp32-t32-reference.json", "execution.json"))
    for path in (log, proof, receipt_path):
        conversion.guard(plan, path, deadline=deadline)
    source = conversion.pin(Path(__file__).resolve(), check)
    argv = [plan["python"]["path"], "-I", "-B", "-S", "-X", "pycache_prefix=" + plan["cache_prefix"],
            source["path"], "--worker", "--deadline-monotonic", repr(deadline), "--worker-sha256", source["sha256"],
            "--stage-receipt", stage_pin["path"], "--stage-receipt-sha256", stage_pin["sha256"]]
    environment = {key: value for key, value in os.environ.items()
                   if not key.upper().startswith(("AIRK_", "ORT_", "PYTHON", "CUDA_", "CUDNN_", "TORCH_", "TRITON_",
                                                  "HF_", "HUGGINGFACE_"))}
    environment.update(CUDA_VISIBLE_DEVICES="", HF_HUB_OFFLINE="1", TRANSFORMERS_OFFLINE="1",
                       TOKENIZERS_PARALLELISM="false", TORCH_HOME=str(temporary),
                       TORCHINDUCTOR_CACHE_DIR=str(temporary), TRITON_CACHE_DIR=str(temporary),
                       TEMP=str(temporary), TMP=str(temporary), TMPDIR=str(temporary))
    host = 32 << 30
    receipt = {"schema": OWNED_SCHEMA, "complete": False, "failure": None, "qualification": False,
               "product_admission": False, "original_deadline_monotonic": deadline, "host_bytes": host,
               "command": argv, "execution_token": token, "worker": source,
               "conversion_plan": plan_pin, "conversion_receipt": prior_pin, "stage_receipt": stage_pin}

    def suspended(api, job, info, child_deadline):
        observed = parent.suspended_authentication(api, info, deadline, child_deadline, conversion.SID,
                                                   plan["python"], owner.image_proof)
        require(conversion.windows_owner(process=info.process) == token
                and owner.job_snapshot(api, job) == ([int(info.pid)], 1, 1), "exact suspended owned process required")
        return dict(observed, token=token)

    try:
        with open(os.devnull, "rb") as stdin, log.open("xb") as stdout:
            conversion.guard(plan, log, True, deadline=deadline)
            receipt["owned"] = owner.run_windows_owned_training(
                argv, ROOT, stdout, deadline - time.monotonic(), environment, stdin, host,
                before_resume=suspended, absolute_deadline=deadline, cleanup_seconds=5)
        owned = receipt["owned"]
        require(parent.settled(owned) and type(owned["peak_host_committed_bytes"]) is int
                and 0 < owned["peak_host_committed_bytes"] <= host,
                f"CPU projection reference failed or did not settle within bounds, exit={owned['exit_code']}")
        conversion.guard(plan, log, True, deadline=deadline)
        require(0 < log.stat().st_size <= MAX_LOG, "bounded projection reference stdout required")
        with log.open("rb") as stream:
            lines = [line[len(MARKER):] for line in stream if line.startswith(MARKER)]
        require(len(lines) == 1 and 0 < len(lines[0]) <= MAX_PROOF, "exact bounded projection worker proof required")
        result = json.loads(lines[0])
        expected_reference, _ = legacy.exact_reference(check)
        reference = dict(result["reference"])
        matching = reference.pop("matching_qkv_fp32")
        require(result["schema"] == SCHEMA and result["complete"] is result["model_inference"] is True
                and result["native_execution"] is result["qualification"] is result["product_admission"] is False
                and result["pid"] == owned["owned_leader_pid"] and result["execution_token"] == token
                and result["original_deadline_monotonic"] == deadline and result["reference_device"] == "cpu"
                and result["worker"] == source and result["stage_receipt"] == stage_pin and result["candidate"] == candidate
                and result["conversion_plan"] == plan_pin and result["conversion_receipt"] == prior_pin
                and result["baseline_candidate"] == {key: stage["baseline_" + key] for key in candidate}
                and result["verification"]["projection_graph"] == stage["graph_proof"]
                and result["recipe"] == {"qkv_fp32_routes": list(ROUTES), "context_flash_single_tile": True}
                and type(result["torch_forward_invocations"]) is int and result["torch_forward_invocations"] == 1
                and reference == expected_reference and legacy.finite_scores(matching),
                "independent projection observation differs from the exact owned request")
        for pin in (plan_pin, prior_pin, stage_pin, source, legacy_pin, graph_source):
            conversion.pinned(pin, check)
        require(not any(temporary.iterdir())
                and {path.name for path in out.iterdir()} == {"temp", log.name}, "unexpected owned reference output")
        with proof.open("xb") as output:
            output.write(lines[0])
        conversion.guard(plan, proof, True, deadline=deadline)
        receipt["proof"] = conversion.pin(proof, check)
        receipt["stdout"] = conversion.pin(log, check)
        receipt["complete"] = True
    except BaseException as error:
        receipt["failure"] = {"type": type(error).__name__, "message": str(error)}
        if hasattr(error, "owned_cleanup"):
            receipt["owned_cleanup"] = error.owned_cleanup
    finally:
        retention = time.monotonic() + 10
        try:
            for path in out.rglob("*"):
                conversion.guard(plan, path, True, deadline=retention)
            if log.is_file() and "stdout" not in receipt and 0 < log.stat().st_size <= MAX_LOG:
                receipt["stdout"] = conversion.pin(log)
        except (OSError, ValueError, TimeoutError) as error:
            receipt["complete"] = False
            receipt["retention_error"] = str(error)
            receipt["failure"] = receipt["failure"] or {"type": type(error).__name__, "message": str(error)}
        parent.final_phase_observation(receipt, deadline, started)
        conversion.guard(plan, receipt_path, deadline=retention)
        with receipt_path.open("x", encoding="utf-8", newline="\n") as output:
            json.dump(receipt, output, allow_nan=False)
            output.write("\n")
        conversion.guard(plan, receipt_path, True, deadline=retention)
    require(receipt["complete"], str(receipt["failure"]))
    return receipt["proof"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage-receipt", type=Path, required=True)
    parser.add_argument("--stage-receipt-sha256", required=True)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--owned-output", type=Path)
    mode.add_argument("--worker", action="store_true")
    parser.add_argument("--worker-sha256")
    parser.add_argument("--deadline-monotonic", type=float)
    args = parser.parse_args()
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "use isolated Windows Python -I -B -S")
    require(args.stage_receipt == STAGE and args.stage_receipt_sha256 == STAGE_SHA,
            "exact reviewed staged projection receipt required")
    if args.owned_output is not None:
        require(args.worker_sha256 is None and args.deadline_monotonic is None, "worker args require owned launch")
        print(json.dumps({"proof": run_owned(args.owned_output), "qualification": False}))
        return
    require(args.worker_sha256 and args.deadline_monotonic and math.isfinite(args.deadline_monotonic),
            "owned worker source and finite original deadline required")
    result = worker(args.deadline_monotonic, args.worker_sha256)
    encoded = json.dumps(result, allow_nan=False, separators=(",", ":")).encode("utf-8")
    require(len(encoded) <= MAX_PROOF and time.monotonic() < args.deadline_monotonic,
            "original projection proof size or deadline exceeded")
    print((MARKER + encoded).decode("utf-8"), flush=True)


if __name__ == "__main__":
    main()
