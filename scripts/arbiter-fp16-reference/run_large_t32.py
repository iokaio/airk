# SPDX-License-Identifier: Apache-2.0
"""Owned CPU check of the baseline Large FP16 graph, weights and one T32 score vector."""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import runpy
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
HERE = Path(__file__).resolve()
PREP = HERE.with_name("prepare_large.py")
SOURCE_RECEIPT = CAMPAIGN / "large-fp16-independent-reference-inputs-r1.json"
PREP_SHA = "cab3b893416bfe13467864de81836c8692c996178345a6361a4adeec2352144c"
RECEIPT_SHA = "c01cd56af84f90a6b842c5318f0c3e8eb757f14a9ce3387752c928c2f835c8c7"
CONVERSION = ROOT / "scripts/arbiter-fp16-execution.py"
CONVERSION_SHA = "025b36292593f6fc944c1774d3faf2e720e3dbd2adbf1c902163b77f84ea3f8f"
STREAM = HERE.with_name("large_publisher.py")
STREAM_SHA = "75df989310931603b3a1fd55cc70dca2e34543638c94ce9520284c1b4eec1c75"
SUPPORT = ROOT / "scripts/arbiter-fp16-execution/verify_compact.py"
SUPPORT_SHA = "50d5c7237cd057a1d08ea79794bbdda0f1e141438a2d22580a760e418f9e3d75"
HOST = 32 << 30
SECONDS = 600
MARKER = "AIRK_LARGE_FP16_T32 "
FLASH_RECIPE = "baseline-half-sm86-causal-flash-single-tile"


def require(value, message):
    if not value:
        raise ValueError(message)


def checked_module(path, digest):
    require(hashlib.sha256(path.read_bytes()).hexdigest() == digest, "maintained source changed: " + path.name)
    return runpy.run_path(str(path))


def own_job():
    import ctypes
    from ctypes import wintypes

    class Basic(ctypes.Structure):
        _fields_ = [("process_time", ctypes.c_longlong), ("job_time", ctypes.c_longlong),
                    ("flags", wintypes.DWORD), ("minimum", ctypes.c_size_t), ("maximum", ctypes.c_size_t),
                    ("active_limit", wintypes.DWORD), ("affinity", ctypes.c_size_t),
                    ("priority", wintypes.DWORD), ("scheduling", wintypes.DWORD)]

    class Extended(ctypes.Structure):
        _fields_ = [("basic", Basic), ("io", ctypes.c_ulonglong * 6), ("process", ctypes.c_size_t),
                    ("job", ctypes.c_size_t), ("peak_process", ctypes.c_size_t), ("peak_job", ctypes.c_size_t)]

    limits = Extended()
    api = ctypes.WinDLL("kernel32", use_last_error=True)
    if not api.QueryInformationJobObject(None, 9, ctypes.byref(limits), ctypes.sizeof(limits), None):
        raise ctypes.WinError(ctypes.get_last_error())
    required_flags = 0x2000 | 0x100 | 0x200 | 0x8
    require(limits.basic.flags & required_flags == required_flags and limits.basic.active_limit == 2
            and limits.process == limits.job == HOST, "original reference Job flags, two process slots and 32 GiB caps required")
    return {"process_bytes": limits.process, "job_bytes": limits.job, "active_limit": limits.basic.active_limit}


def retained_prerequisites(c, plan, check):
    """Bind this CPU comparison to the source epoch that produced the retained conversion."""
    evidence = c["read"](c["pinned"](plan["prerequisites_manifest"], check))
    require(evidence["schema"] == "airk.private.fp16-prerequisites/2" and evidence["model"] == "large"
            and evidence["first_fp32"]["sha256"] == c["FP32"]["large"]["scoring"]
            and evidence["resource_fp32"]["sha256"] == c["FP32"]["large"]["resource"],
            "Large conversion-time FP32 prerequisites differ")
    groups = []
    for key, kind in (("native_g5", "tests"), ("native_g5_clippy", "clippy")):
        proof = c["read"](c["pinned"](evidence[key], check))
        c["successful_readiness"](proof, kind)
        groups.append(proof["groups"]["native"])
        if kind == "tests":
            junit = proof["junit"]
            require(Path(junit["path"]).name == junit["path"], "retained G5 JUnit must be a sibling leaf")
            path = Path(evidence[key]["path"]).parent / junit["path"]
            require(c["pin"](path, check)["sha256"] == junit["sha256"], "retained G5 JUnit drift")
            c["validate_junit"](path.read_bytes(), proof["selected_tests"])
    inputs = {r["path"]: r["sha256"] for r in plan["input_files"]}
    require(groups[0] == groups[1] and all(groups[0]["files"][key] == inputs.get(str(ROOT / key))
            for key in c["G5_SOURCES"]), "G5 source epoch differs from the sealed Large inputs")


def authenticate(args, deadline):
    def check():
        if time.monotonic() >= deadline:
            raise TimeoutError("Large independent reference deadline expired")

    check()
    support = checked_module(SUPPORT, SUPPORT_SHA)
    c = checked_module(CONVERSION, CONVERSION_SHA)
    token = c["windows_owner"]()
    plan, pp = support["read_pinned_json"](args.plan, args.plan_sha256, check)
    receipt, rp = support["read_pinned_json"](args.conversion, args.conversion_sha256, check)
    meta, mp = support["read_pinned_json"](SOURCE_RECEIPT, RECEIPT_SHA, check)
    require(plan["schema"] == c["SCHEMA"] and plan["model"] == "large"
            and plan["repository_root"] == str(ROOT) and plan["human_sid"] == c["SID"]
            and plan["source_plan"]["sha256"] == c["SOURCE_PLAN_SHA"]
            and plan["limits"] == c["LIMITS"] and plan["expected_attention_count"] == 36
            and plan["expected_weights_bytes"] == meta["memory_components"]["matching_reference_weight_bytes"]
            and plan["wrapper"] == c["pin"](CONVERSION, check) and c["pin"](sys.executable, check) == plan["python"]
            and plan["source_graph"]["sha256"] == "f8ba5d3ea8a4269739ec5e517e61e4ec1f2f94560ea60014a4f5a9476c32fd94",
            "reviewed Large conversion plan and exact interpreter required")
    prefix = Path(plan["cache_prefix"])
    require(prefix.is_absolute() and prefix.is_relative_to(CAMPAIGN) and not prefix.exists()
            and prefix.resolve(strict=False) == prefix, "absent campaign cache prefix required")
    require(plan["guard"]["path"] == str(ROOT / "scripts/dev/assert-human-execution.ps1")
            and plan["guard"]["sha256"] == "57592dbbbefc63030c97a1d3485bc5c4fda7c2243d792b7399432b129ea4c57f"
            and plan["powershell"]["path"] == str(Path("C:/Program Files/PowerShell/7/pwsh.exe"))
            and plan["powershell"]["sha256"] == "e94166ed9f376559c1359bafb08bc49e463a494c330f67644ec1d9f1ffeb5ee5"
            and plan["exporter"]["sha256"] == c["CONTEXT_PRESENT_EXPORTER_SHA"], "reviewed execution guard or exporter differs")
    require(args.plan.is_relative_to(CAMPAIGN) and args.conversion == Path(plan["output_root"]) / "execution.json"
            and args.conversion.is_relative_to(CAMPAIGN) and receipt["schema"] == "airk.private.fp16-conversion-execution/1"
            and receipt["complete"] is True and receipt["failure"] is None and receipt["final_errors"] == []
            and receipt["qualification"] is receipt["activation"] is receipt["model_inference"] is False
            and receipt["whole_phase_expired"] is False and receipt["limits"] == plan["limits"]
            and 0 <= receipt["elapsed_seconds"] < plan["limits"]["wall_seconds"]
            and receipt["plan"] == pp, "completed Large conversion required")
    retained_prerequisites(c, plan, check)
    proof = receipt["final_proofs"]
    require(set(proof) == set(c["PROOFS"]) and proof["inputs"] == pp
            and receipt["observed_proofs"] == {k: v for k, v in proof.items() if k != "inputs"},
            "incomplete Large conversion proof roster")
    base = args.conversion.parent
    for key, name in (("graph", "candidate/model.onnx"), ("weights", "candidate/weights.bin"),
                      ("manifest", "candidate/export-manifest.private.json"), ("worker", "worker.json"),
                      ("stdout", "stdout.log")):
        require(proof[key]["path"] == str(base / name), "converted file location differs")
        c["windows_owner"](base / name)
        c["pinned"](proof[key], check)
    require({p.name for p in (base / "candidate").iterdir()} == c["OUTPUT_NAMES"], "candidate closure differs")
    c["validate_manifest"](c["read"](proof["manifest"]["path"]), plan, proof["graph"], proof["weights"])
    helpers = ROOT / "scripts/cuda-native-execution/helpers"
    require(set(plan["helpers"]) == set(c["HELPERS"]), "helper roster differs")
    for leaf, sha in c["HELPERS"].items():
        row = plan["helpers"][leaf]
        require(row["path"] == str(helpers / leaf) and row["sha256"] == sha, "ownership helper identity differs")
        c["pinned"](row, check)
    sys.path.insert(0, str(helpers))
    parent = support["load_module"](helpers / "reference_parent_no_site_r2.py", "large_reference_parent")
    owner = support["load_module"](helpers / "reference_owned.py", "large_reference_owner")
    require(Path(parent.worker.__file__) == helpers / "reference_worker.py", "shadowed ownership helper")
    owned = receipt["owned"]
    pid = owned["owned_leader_pid"]
    require(parent.settled(owned) and 0 < owned["peak_host_committed_bytes"] <= plan["limits"]["host_committed_bytes"]
            and owned["suspended_assignment"]["pid"] == pid
            and owned["suspended_assignment"]["token"] == token
            and owned["suspended_assignment"]["execution_user_sid"] == c["SID"]
            and owned["suspended_assignment"]["image"] == c["core_pin"](plan["python"]),
            "conversion process authentication or settlement differs")
    worker = c["read"](proof["worker"]["path"])
    require(worker["complete"] is True and worker["pid"] == pid and worker["token"] == token
            and worker["model_inference"] is worker["qualification"] is False
            and receipt["command"] == [plan["python"]["path"], "-B", "-S", "-X", "pycache_prefix=" + plan["cache_prefix"],
                str(CONVERSION), "--worker", "--plan", str(args.plan), "--plan-sha256", pp["sha256"], "--deadline",
                repr(receipt["original_deadline_monotonic"])], "conversion worker or original command differs")
    for path in (args.plan, args.conversion, base, base / "candidate", SOURCE_RECEIPT):
        c["windows_owner"](path)
    return c, support, plan, pp, rp, proof, meta, mp, parent, owner, token, check


def reference(args):
    limits = own_job()
    require(0 < args.deadline - time.monotonic() <= SECONDS and os.environ.get("CUDA_VISIBLE_DEVICES") == ""
            and sys.pycache_prefix and not Path(sys.pycache_prefix).exists(), "bounded CPU-only worker required")
    c, support, plan, pp, rp, proofs, meta, mp, _, _, token, check = authenticate(args, args.deadline)
    require(sys.pycache_prefix == plan["cache_prefix"] and c["pin"](HERE, check)["sha256"] == args.worker_sha256,
            "worker source or cache identity differs")
    cache = Path(os.environ["TORCHINDUCTOR_CACHE_DIR"])
    require(cache.is_relative_to(CAMPAIGN) and cache.is_dir() and cache.resolve(strict=True) == cache, "private cache required")
    c["windows_owner"](cache)
    sys.addaudithook(lambda event, arguments: support["read_only"](event, arguments, cache))
    inputs = {r["path"]: r for r in plan["input_files"]}
    runtime = c["read"](c["pinned"](plan["runtime_manifest"], check))
    original, selected = c["source_facts"](plan["source_plan"], "large", check)
    require(selected["original_graph"] == plan["source_graph"] and selected["source_closure"] == plan["source_closure"],
            "original Large graph lineage differs")
    home = Path(sys.executable).parent
    require(runtime["roots"] == runtime["import_paths"] == [str(home / "Lib"), str(home / "DLLs"), original["python_site"]]
            and all(inputs.get(k) == v and k == v["path"] for k, v in runtime["files"].items()),
            "sealed runtime closure differs")
    prep = checked_module(PREP, PREP_SHA)
    require(prep["prepare"]() == meta, "Large publisher or reference input observation changed")
    for p in (HERE, PREP, SUPPORT, STREAM, *(ROOT / path for path in prep["SOURCES"])):
        inputs[str(p)] = c["pin"](p, check)
    sys.path.extend(runtime["import_paths"])
    import onnx
    import torch
    support["loaded_runtime"](inputs, c["pin"], check)
    torch.set_num_threads(2)
    torch.set_num_interop_threads(1)
    half = support["load_module"](ROOT / "scripts/arbiter-fp16-reference.py", "large_independent_half")
    def observe(stage, detail=None):
        print("AIRK_LARGE_REFERENCE_PHASE " + json.dumps({"stage": stage, "detail": detail}), flush=True)
        check()
    streamer = checked_module(STREAM, STREAM_SHA)
    independent, publisher = streamer["build"](half, torch, prep, observe)
    observe("publisher_parameters_ready")
    graph = Path(proofs["graph"]["path"])
    weight = Path(proofs["weights"]["path"])
    verification = half.verify_actual_half_parameters(independent, graph, onnx, {weight.name: weight},
        candidate_sha256=proofs["graph"]["sha256"], original_graph=Path(plan["source_graph"]["path"]),
        original_files=c["read"](plan["source_closure"]["path"]))
    check()
    observe("graph_and_learned_bytes_verified")
    row = meta["rows"][0]
    require(len(row["input"]["input_ids"]) == 32, "T32 reference required")
    scores = independent.forward(row["input"], context_flash_single_tile=args.recipe == FLASH_RECIPE)
    require(scores.dtype == torch.float32 and scores.device.type == "cpu" and tuple(scores.shape) == (1, 8)
            and not torch.cuda.is_initialized(), "CPU-only T32 score ABI differs")
    observe("t32_forward_completed")
    vector = scores[0].tolist()
    require(support["finite_scores"](vector), "nonfinite CPU reference")
    for key in ("graph", "weights", "manifest"):
        c["pinned"](proofs[key], check)
    # Repeat source and candidate custody after the actual forward on the same deadline.
    for item in (pp, rp, mp, plan["source_graph"], plan["source_closure"], *meta["publisher_files"].values()):
        c["pinned"](item, check)
    for name, item in c["read"](plan["source_closure"]["path"]).items():
        c["pinned"](dict(item, path=str(Path(plan["source_graph"]["path"]).parent / name)), check)
    for item in meta["sources"]:
        require(c["pin"](item["path"], check)["sha256"] == item["sha256"], "independent source changed")
    for root, fixture, producer in prep["PAIRS"]:
        support["read_pinned_json"](root / "numeric-fixture.json", fixture, check)
        support["read_pinned_json"](root / "reference-producer.json", producer, check)
    return {"model": "large", "recipe": args.recipe, "conversion_plan": pp, "conversion_receipt": rp,
            "source_inputs": mp, "graph": proofs["graph"], "weights": proofs["weights"], "verification": verification,
            "publisher": publisher, "streamed_loader": inputs[str(STREAM)], "input_digest": row["input"]["input_digest"], "original_fp32": row["reference_original_fp32"],
            "matching_fp16": vector, "loaded_runtime": support["loaded_runtime"](inputs, c["pin"], check),
            "forward_count": 1, "job": limits, "device": "cpu", "pid": os.getpid(), "token": token, "deadline": args.deadline,
            "native_execution": False, "qualification": False}


def run(args):
    start = time.monotonic()
    deadline = start + SECONDS
    c, support, plan, pp, rp, proofs, meta, mp, parent, owner, token, check = authenticate(args, deadline)
    out = args.owned_output
    require(out.parent == CAMPAIGN and out.resolve(strict=False) == out and not out.exists(), "fresh private output required")
    temp = out / "temp"
    for path in (out, temp):
        c["guard"](plan, path, deadline=deadline)
        path.mkdir()
        c["guard"](plan, path, True, deadline=deadline)
    log, evidence, receipt = (out / name for name in ("stdout.log", "reference.json", "execution.json"))
    for path in (log, evidence, receipt):
        c["guard"](plan, path, deadline=deadline)
    source = c["pin"](HERE, check)
    argv = [sys.executable, "-I", "-B", "-S", "-X", "pycache_prefix=" + plan["cache_prefix"], str(HERE),
            "--worker", "--plan", str(args.plan), "--plan-sha256", pp["sha256"], "--conversion", str(args.conversion),
            "--conversion-sha256", rp["sha256"], "--deadline", repr(deadline), "--worker-sha256", source["sha256"],
            "--recipe", args.recipe]
    env = {k: v for k, v in os.environ.items() if not k.upper().startswith(
        ("AIRK_", "ORT_", "PYTHON", "CUDA_", "CUDNN_", "TORCH_", "TRITON_", "HF_", "HUGGINGFACE_"))}
    env.update(CUDA_VISIBLE_DEVICES="", HF_HUB_OFFLINE="1", TRANSFORMERS_OFFLINE="1", TOKENIZERS_PARALLELISM="false",
               TORCH_HOME=str(temp), TORCHINDUCTOR_CACHE_DIR=str(temp), TRITON_CACHE_DIR=str(temp),
               TEMP=str(temp), TMP=str(temp), TMPDIR=str(temp))
    result = {"schema": "airk.private.owned-large-fp16-t32-reference/1", "complete": False, "qualification": False, "failure": None,
              "plan": pp, "conversion": rp, "worker": source, "command": argv, "host_bytes": HOST, "deadline": deadline}

    def suspended(api, job, info, child_deadline):
        observation = parent.suspended_authentication(api, info, deadline, child_deadline, c["SID"], plan["python"], owner.image_proof)
        require(c["windows_owner"](process=info.process) == token and owner.job_snapshot(api, job) == ([int(info.pid)], 1, 1),
                "worker owner or suspended Job membership differs")
        return dict(observation, token=token)

    try:
        with open(os.devnull, "rb") as stdin, log.open("xb") as stdout:
            result["owned"] = owner.run_windows_owned_training(argv, ROOT, stdout, deadline-time.monotonic(), env,
                stdin, HOST, before_resume=suspended, absolute_deadline=deadline, cleanup_seconds=5)
        owned = result["owned"]
        require(parent.settled(owned) and 0 < owned["peak_host_committed_bytes"] <= HOST, "unsettled/failed Large CPU reference")
        c["guard"](plan, log, True, deadline=deadline)
        require(0 < log.stat().st_size <= 64 << 20 and not any(temp.iterdir()), "log, output or cache bound differs")
        require({path.name for path in out.iterdir()} == {"temp", "stdout.log"}, "unexpected Large reference output")
        with log.open("rb") as stream:
            matches = [line[len(MARKER):] for line in stream if line.startswith(MARKER.encode())]
        require(len(matches) == 1 and len(matches[0]) <= 16 << 20, "one bounded private score observation required")
        obs = json.loads(matches[0])
        require(obs["conversion_plan"] == pp and obs["conversion_receipt"] == rp and obs["source_inputs"] == mp
                and obs["graph"] == proofs["graph"] and obs["weights"] == proofs["weights"]
                and obs["pid"] == owned["owned_leader_pid"] and obs["token"] == token and obs["deadline"] == deadline
                and obs["model"] == "large" and obs["recipe"] == args.recipe and obs["device"] == "cpu"
                and obs["job"] == {"process_bytes": HOST, "job_bytes": HOST, "active_limit": 2}
                and obs["forward_count"] == 1 and obs["original_fp32"] == meta["rows"][0]["reference_original_fp32"]
                and obs["input_digest"] == meta["rows"][0]["input"]["input_digest"] and support["finite_scores"](obs["matching_fp16"])
                and obs["native_execution"] is obs["qualification"] is False, "Large CPU proof differs from owned request")
        for entry in (pp, rp, mp, source):
            c["pinned"](entry, check)
        with evidence.open("xb") as file:
            file.write(matches[0])
        c["guard"](plan, evidence, True, deadline=deadline)
        result.update(proof=c["pin"](evidence, check), stdout=c["pin"](log, check), complete=True)
    except BaseException as error:
        result["failure"] = str(error)
        if hasattr(error, "owned_cleanup"):
            result["owned_cleanup"] = error.owned_cleanup
    finally:
        parent.final_phase_observation(result, deadline, start)
        retention = time.monotonic() + 10
        for path in out.rglob("*"):
            c["guard"](plan, path, True, deadline=retention)
        c["guard"](plan, receipt, deadline=retention)
        with receipt.open("x", encoding="utf-8") as file:
            json.dump(result, file, indent=2, allow_nan=False)
        c["guard"](plan, receipt, True, deadline=retention)
    require(result["complete"], str(result.get("failure")))
    return result["proof"]


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", required=True, type=Path)
    parser.add_argument("--plan-sha256", required=True)
    parser.add_argument("--conversion", required=True, type=Path)
    parser.add_argument("--conversion-sha256", required=True)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--owned-output", type=Path)
    mode.add_argument("--worker", action="store_true")
    parser.add_argument("--deadline", type=float)
    parser.add_argument("--worker-sha256")
    parser.add_argument("--recipe", choices=("baseline-half", FLASH_RECIPE), default="baseline-half")
    args = parser.parse_args()
    require(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode, "run with -I -B -S")
    if args.worker:
        require(args.deadline is not None and math.isfinite(args.deadline), "original bounded deadline required")
        print(MARKER + json.dumps(reference(args), allow_nan=False), flush=True)
    else:
        print(json.dumps(run(args), allow_nan=False))
