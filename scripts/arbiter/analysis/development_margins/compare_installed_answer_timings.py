# SPDX-License-Identifier: Apache-2.0
"""Reconcile two original installed Answer arms and their twenty chronological CLI observations."""
import argparse
import json
from pathlib import Path
import runpy

HERE = Path(__file__).resolve().parent
owner = runpy.run_path(str(HERE / "installed_answer_timing.py"))
require, pin, obs = owner["require"], owner["pin"], owner["obs"]


def reopen(path, seal, spec_path, expected_provider):
    require(path.parent.parent == owner["E"] and path.name == "arm.json", "original campaign arm required")
    result = pin(path, seal)
    arm = pin(spec_path, result["arm_sha256"])
    roster = pin(owner["ROSTER"], owner["ROSTER_SHA"])
    require(result["roster_sha256"] == owner["ROSTER_SHA"] and result["provider"] == arm["provider"] == expected_provider
            and result["model"] == arm["model"] and result["context_manifest"] == arm["context_manifest"]
            and result["calibration_sha256"] == arm["calibration_sha256"], "paired arm identity differs")
    install = pin(path.parent / "install-record.json", arm["install_record_sha256"])
    cli = str(owner["INSTALL"].parent / install["executables"]["airkctl"]["path"])
    if "active" in result:
        def status(label):
            meta = json.loads((path.parent / (label + ".command.json")).read_bytes())
            stdout = path.parent / (label + ".stdout")
            require(meta["argv"][0] == cli and meta["argv"][1:] == ["--output","json","--service-runtime-dir",arm["control"],"arbiter","status"]
                    and meta["state"] == "observed" and meta["exit_code"] == 0 and meta["stdout"] == str(stdout),
                    "different product status command")
            return obs["stages"](pin(stdout, meta["stdout_sha256"]), arm["indexes"].values(), arm, roster["cases"][0]["focus_profile"])
        require(status("stage-before") == result["active"], "sealed preflight status differs")
        if result["terminal"] == "settled":
            require(status("after-19") == result["active"], "sealed terminal status differs")
    cases = {r["case"]:r for r in roster["cases"]}
    order = roster["warm_sample_order"]
    seen = set()
    for row in result["rows"]:
        i = row["ordinal"]
        case = cases[row["case"]]
        require(order[i] == case["case"] and row["request_sha256"] == case["request_sha256"], "original request ordinal differs")
        if row["state"] == "deferred":
            require(row["elapsed_ns"] is None and "command" not in row, "deferred row has invented work")
            continue
        label = f"warm-{i:02}"
        require(row["command"] == label + ".command.json", "source command is not the measured ordinal")
        command = json.loads((path.parent / row["command"]).read_bytes())
        index = arm["indexes"][case["case"]]
        require(command["argv"][0] == cli and command["argv"][1:] == ["--output","json","--service-runtime-dir",arm["control"],
                "tools","call","airk_answer","--index",index,"--args-file",case["request"]]
                and command["end_ns"] - command["start_ns"] == command["elapsed_ns"] == row["elapsed_ns"]
                and Path(command["stdout"]) == path.parent / (label+".stdout"), "original native timer, source or scope differs")
        raw, _ = owner["freeze"]["sealed"](Path(command["stdout"]),command["stdout_sha256"])
        if command["state"] == "observed":
            require(command["exit_code"] == 0,"observed request lacks a successful native command")
            request = pin(Path(case["request"]),case["request_sha256"])
            derived = owner["classify_stdout"](raw,result["scopes"][case["case"]],index,request,
                                               case["focus_profile"],arm,command["elapsed_ns"])
            require({k:row[k] for k in ("state","native","error") if k in row} == derived,
                    "reported disposition differs from original Answer or elapsed deadline")
            if "native" in derived:
                request_id = derived["native"]["request_id"]
                require(request_id not in seen,"native request identity reused")
                seen.add(request_id)
        else:
            require(row["state"] == command["state"] and row["state"] in ("timeout","cli_error")
                    and "native" not in row,"unknown or contradictory measured disposition")
    stats = obs["summary"](result["rows"], order)
    require(stats == result["summary"], "recomputed service p95 differs")
    return result, stats


def selected_inference(row, provider):
    return [e["advice"]["input_digest"] for e in row.get("native", {}).get("context", [])
            if row["state"] == "observed" and e["status"] == "selected" and e.get("advice")
            and e["advice"]["outcome"]["status"] == "accepted" and e["advice"]["runtime"] == "onnxruntime-" + provider + "/1.30.0"
            and (e["advice"]["observed_input_tokens"] or 0) > 0 and (e["advice"]["work"]["inference_us"] or 0) > 0]


def large_absolute(gpu):
    run, stats = gpu
    require(run["provider"] == "cuda" and run["model"] == "context_large", "absolute ceiling applies to CUDA Large only")
    ready = run.get("active")
    qualified_cpu = None if ready is None else ready["qualified_same_model_cpu_variants"]
    gpu_exercised = sum(bool(selected_inference(r, "cuda")) for r in run["rows"])
    complete = run["terminal"] == "settled" and stats["complete"] == 20 and stats["deadline_overruns"] == 0
    status = "incomplete_workload"
    if qualified_cpu is None:
        status = "missing_catalog_evidence"
    elif qualified_cpu:
        status = "requires_qualified_cpu_comparison"
    elif complete:
        status = "within_absolute_ceiling" if stats["p95_elapsed_ns"] <= 120_000_000_000 else "exceeds_absolute_ceiling"
    return {"model":run["model"], "provider":"cuda", "summary":stats, "absolute_limit_ns":120_000_000_000,
            "native_catalog_qualified_cpu_variants":qualified_cpu, "complete_workload_absolute_status":status,
            "cuda_selected_inference_rows":gpu_exercised,
            "context_absolute_latency_supported":status == "within_absolute_ceiling" and gpu_exercised > 0,
            "paired_task_quality_measured":False}


def compare(cpu, gpu):
    a, x = cpu
    b, y = gpu
    require(a["model"] == b["model"] and a["provider"] == "cpu" and b["provider"] == "cuda"
            and a["roster_sha256"] == b["roster_sha256"] and a["install_record_sha256"] == b["install_record_sha256"],
            "paired requests require the same installed release, model and workload")
    rows = []
    for ca, ga in zip(a["rows"], b["rows"], strict=True):
        require(ca["case"] == ga["case"] and ca["ordinal"] == ga["ordinal"]
                and ca["request_sha256"] == ga["request_sha256"], "mismatched matched workload")
        left, right = selected_inference(ca, "cpu"), selected_inference(ga, "cuda")
        rows.append({"case": ca["case"], "ordinal": ca["ordinal"], "cpu_elapsed_ns": ca["elapsed_ns"], "cuda_elapsed_ns": ga["elapsed_ns"],
                     "cpu_state":ca["state"], "cuda_state":ga["state"], "selected_cpu":len(left), "selected_cuda":len(right),
                     "matched_selected_inputs":bool(left) and left == right})
    complete = (a["terminal"] == b["terminal"] == "settled" and x["complete"] == y["complete"] == 20
                and x["deadline_overruns"] == y["deadline_overruns"] == 0)
    p, q = x["p95_elapsed_ns"], y["p95_elapsed_ns"]
    saved = p-q if p is not None and q is not None else None
    meets_time = complete and saved >= 1_000_000_000 and saved >= .25*p
    matched = sum(r["matched_selected_inputs"] for r in rows)
    return {"model":a["model"], "cpu":x, "cuda":y, "saved_p95_ns":saved,
            "p95_fraction_saved":saved / p if saved is not None and p else None, "complete_workload_time_target_met":meets_time,
            "context_speed_evidence_status":"unmeasured_no_matched_selected_inference" if not matched else
                    ("matched_workload_target_met" if meets_time else "matched_workload_target_not_met"),
            "matched_selected_context_rows":sum(r["matched_selected_inputs"] for r in rows), "rows":rows,
            "paired_task_quality_measured":False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--large-absolute", action="store_true")
    for provider in ("cpu","cuda"):
        parser.add_argument("--"+provider, type=Path, required=provider == "cuda")
        parser.add_argument("--"+provider+"-sha256", required=provider == "cuda")
        parser.add_argument("--"+provider+"-spec", type=Path, required=provider == "cuda")
    args = parser.parse_args()
    gpu = reopen(args.cuda,args.cuda_sha256,args.cuda_spec,"cuda")
    supplied = (args.cpu, args.cpu_sha256, args.cpu_spec)
    if args.large_absolute:
        require(not any(supplied), "CPU comparison and Large-only observation are separate modes")
        result = large_absolute(gpu)
    else:
        require(all(supplied), "matched CPU receipt, digest and arm specification required")
        result = compare(reopen(*supplied,"cpu"), gpu)
    print(json.dumps(result, sort_keys=True, allow_nan=False))
