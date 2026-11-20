# SPDX-License-Identifier: Apache-2.0
"""Score the frozen TechQA TRAIN request order with independent Compact CPU references."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import re
import runpy
import struct
import sys
import time


HERE = Path(__file__).resolve().parent
MARGINS = HERE.parent / "arbiter/analysis/development_margins"
CAMPAIGN = HERE.parents[1] / "artifacts/v190/final-plan/exec-20261116T052942Z"
CENSUS = CAMPAIGN / "compact-development-focus1024-final-census-r1.private.json"
PREFIX = CAMPAIGN / "compact-development-focus1024-prefix12-census-r1.private.json"
ORIGINAL_PREFIX_SHA = "a30258f4d43fa8d4822a0c56c93c9726448f7eec3ed4bdb72148ba436120bfe7"
Q543_CAPTURE = "0eb6a5b4f433e9022179a1c950095f1466e9d8ca0b111ecd63ea2a0d7eeaaa47"
AFFINITY = MARGINS / "observe_cal_thread_affinity.py"
SOURCES = {
    MARGINS / "grouped_reuse.py": "8588e26901dc869a955949083aa0580abca727e735a22166a9ecd81c4dbea667",
    AFFINITY: "7f37dcc3f9be734a964f561d9afc68b54ede19d036094b7fb7dab697f7d3f57f",
    HERE / "representative_compact_reference.py": "e574ed3d2b7dd3f736891bcf5e40c2eb45edc43f1fbb73bec902c330aded83e1",
    HERE / "replay_compact_t1024_suffix.py": "03f2170fa7112396bfb861ea486763371b2ca8707d4bb2e060d72af902d21e59",
    MARGINS / "collect_focus1024_train.py": "3db00fd90ce82d4c9b083d0c9e9794164f16fccc358499088acb50ebbca8edf5",
    MARGINS / "capture_inputs.py": "461bc8b81fc13d604ceccc086cd6e9a9339968d7a474fa4bf206c7ea5d176248",
    MARGINS / "focus1024.py": "428db7bb0e22f96bb6e67b3c9f24276cd9cdca4be92563a05881702a3aecf211",
}
Q111_COMPARISON = "05ccbf49749cc814063b88ec759fe61cacd9f2e51e6798dc715df6c625a4cce0"
Q111_REFERENCE = "f657bbcdc9dac88829376c639bf2f20b6e4ae4c40f796f6305c1ec044bb59b91"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def batch_entries(cohort, roster, batch, q543_only=False):
    start = (batch - 1) * 3
    end = start + 3
    if q543_only:
        require(batch == 5 and [r["case"] for r in cohort["rows"][12:]] == ["TRAIN_Q543", "TRAIN_Q564", "TRAIN_Q598"]
                and [r["state"] for r in cohort["rows"][12:]] == ["admitted", "unqualified_capture", "unobserved"]
                and cohort["rows"][12]["capture_sha256"] == Q543_CAPTURE, "original Q543 cutoff differs")
        end = start + 1
    require(1 <= batch <= 5 and len(cohort["rows"]) == len(roster) == 15
            and [row["case"] for row in cohort["rows"]] == [row["case"] for row in roster]
            and all(row["state"] in {"admitted", "budget_stop_zero", "unqualified_capture"} for row in cohort["rows"][start:end]),
            "settled chronological cohort required")
    return list(zip(roster[start:end], cohort["rows"][start:end], strict=True))


def bounds(scores, mask):
    require(len(scores) == len(mask) and all(type(v) is bool for v in mask) and any(mask)
            and all(math.isfinite(v) for v in scores), "finite scores and native option mask required")
    live = sorted((i for i, allowed in enumerate(mask) if allowed), key=lambda i: scores[i], reverse=True)
    margin = scores[live[0]] - scores[live[1]] if len(live) > 1 else None
    return {"winner": live[0], "live_options": len(live), "top_two_margin": margin,
            "within_0014": margin is not None and margin <= .014, "within_0020": margin is not None and margin <= .020,
            "within_0140": margin is not None and margin <= .140}


def checked_sources():
    for path, sha in SOURCES.items():
        require(hashlib.sha256(path.read_bytes()).hexdigest() == sha, "source changed: " + path.name)
    return runpy.run_path(str(HERE / "representative_compact_reference.py")), runpy.run_path(str(MARGINS / "collect_focus1024_train.py"))


def inputs(args, base, census, legacy, check):
    require(re.fullmatch("[0-9a-f]{64}", args.census_sha256), "exact closed census SHA required")
    q543_only = args.batch == 5 and args.census_sha256 == ORIGINAL_PREFIX_SHA
    path = PREFIX if args.batch <= 4 or q543_only else CENSUS
    cohort, cohort_pin = legacy.read_pinned_json(path, args.census_sha256, check)
    binding = census["inputs"]
    roster = binding["load"](binding["INPUTS"] / "roster.private.json", census["ROSTER_SHA"])
    require(cohort["active_focus_manifest"] == census["focus"]["FOCUS"] and cohort["roster_sha256"] == census["ROSTER_SHA"],
            "unexpected source roster or Focus identity")
    selected = []
    for source, summary in batch_entries(cohort, roster, args.batch, q543_only):
        require(census["collect"](source) == summary, "capture or native source seals changed")
        case = source["case"]
        run = Path(summary["run_directory"])
        admissions = []
        for item in summary.get("admissions", []):
            record, actual_pin = legacy.read_pinned_json(run / "fixture/context-cal-admissions" / item["file"], item["sha256"], check)
            require(record["ordinal_in_private_run"] == item["ordinal"] and not record["Context_NN_dispatch"]
                    and record["actual_Focus_profile"] == census["focus"]["FOCUS"], "unverified native ordinal or Focus")
            typed, = [r for r in record["compiler_census"] if r["model"] == base["COMPACT"]]
            req = record["request"]
            require(typed["profile"] == base["CAPTURE_PROFILE"] and typed["NN_invocations"] == 0, "unexpected compiler or model forward")
            admission = {"ordinal": item["ordinal"], "capture": actual_pin, "actual_focus_forward": record["actual_Focus_forward_observed"],
                         "capture_focus": record["actual_Focus_profile"], "actual_context": record["actual_Context_profile"]["manifest"],
                         "profile": typed["profile"], "scope": req["scope"],
                         "offered_choices": req["choices"], "roster": req["roster"], "remaining_input_tokens": req["remaining_input_tokens"],
                         "remaining_ns": record["remaining_ns_at_observation"], "parent_budget": req["parent_budget"]}
            if "compiler_refusal" in typed or not typed["fits_remaining_allowance"]:
                admission.update(state="native_compiler_refused_or_exceeds_allowance", compiler_census=typed)
            else:
                row = typed["compiled"]
                require(0 < len(row["input_ids"]) == typed["input_tokens"] <= req["remaining_input_tokens"]
                        and len(row["candidate_positions"]) == len(req["roster"]["cards"]), "compiled input or remaining budget differs")
                admission.update(state="eligible", compiled=row)
            admissions.append(admission)
        selected.append({"case": case, "capture_state": summary["state"], "case_evidence": summary, "admissions": admissions})
    return cohort_pin, selected


def reuse_q543(receipt, case, admission, proof, proof_pin, runtime_pin):
    run, result = receipt["owned"], receipt["result"]
    previous, = result["rows"]
    prior, = previous["admissions"]
    require(receipt["complete"] and run["exit_code"] == 0 and run["owned_descendants_settled"]
            and run["owned_job_final_active_processes"] == 0 and run["owned_job_final_member_pids"] == []
            and not run["timed_out"] and not run["owned_job_forced_termination"] and result["batch"] == 5
            and result["cohort"]["sha256"] == ORIGINAL_PREFIX_SHA and result["reference_basis"] == proof_pin
            and result["runtime"] == runtime_pin and result["graph_sha256"] == proof["candidate"]["graph"]["sha256"]
            and result["ragged_t65_constant"] and result["matching_t128_words_equal"] and result["original_t128_max_abs"] <= .001
            and case["case"] == previous["case"] == "TRAIN_Q543" and previous["case_evidence"] == case["case_evidence"]
            and previous["case_evidence"]["capture_sha256"] == Q543_CAPTURE and len(case["admissions"]) == 1
            and admission["state"] == "eligible" and prior["state"] == "scored"
            and all(prior[k] == v for k, v in admission.items() if k != "state"), "Q543 source or numerical identity differs")
    return prior["matching_fp16"], prior["original_fp32"]


def score(args, replay, wider, base, census):
    placement = None
    if args.cal_pid is not None:
        placement = runpy.run_path(str(AFFINITY))["place_optional_worker"](args.cal_pid, wider.WIDER_CPU_THREADS)
    def check():
        require(time.monotonic() < args.worker_deadline, "original 600-second batch budget exhausted")

    owner = base["owner"]
    legacy = replay.read_only_bootstrap()
    cohort, selected = inputs(args, base, census, legacy, check)
    proof, proof_pin = legacy.read_pinned_json(CAMPAIGN / "compact-fp16-qkv-fp32-wider-reference-r1" / wider.WIDER_LEAF,
                                             owner.REFERENCE_SHA, check)
    runtime, runtime_pin = legacy.read_pinned_json(replay.RUNTIME, replay.RUNTIME_SHA, check)
    for member in proof["publisher_files"].values():
        require(replay.file_pin(Path(member["path"]), check) == member, "publisher model bytes changed")
    before = set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import torch
    torch.set_num_threads(wider.WIDER_CPU_THREADS)
    torch.set_num_interop_threads(1)
    half = owner.module("../arbiter-fp16-reference.py", legacy.REFERENCES[legacy.REFERENCE_OWNER])
    pub = owner.module("../" + legacy.PUBLISHER_OWNER.removeprefix("scripts/"), legacy.REFERENCES[legacy.PUBLISHER_OWNER])
    half.freeze_reference_math(torch)
    publisher, config = pub.load_authenticated(legacy.PUBLISHER_ROOT, "Qwen/Qwen3-1.7B")
    model = half.HalfReference(torch, config, publisher.text.state_dict(), False, "cpu")
    control = proof["references"][0]
    require(len(control["input"]["input_ids"]) == 128, "T128 regression input differs")
    with torch.inference_mode():
        q = torch.zeros((1, 16, 65, 128), dtype=torch.float16)
        k = torch.zeros((1, 8, 65, 128), dtype=torch.float16)
        test = base["ragged_flash"](torch, wider, q, k, torch.ones_like(k), [1] * 65, .125, check)
        require(torch.equal(test, torch.ones_like(test)), "T65 ragged control differs")
        baseline = base["matching"](half, wider, model, control["input"], check)
        require(struct.pack("<8f", *baseline) == struct.pack("<8f", *control["matching_qkv_fp32"]), "T128 matching words differ")
        original = base["publisher_scores"](torch, publisher, control["input"])
        error = max(abs(x - y) for x, y in zip(original, control["original_fp32"], strict=True))
        require(error <= .001, "T128 original FP32 regression ceiling exceeded")
        for case in selected:
            for admission in case["admissions"]:
                check()
                if admission["state"] != "eligible":
                    continue
                row = admission["compiled"]
                width = len(row["input_ids"])
                if row["attention_mask"] != [1] * width or not 0 < width <= 8192:
                    admission.update(state="cpu_reference_refused", reason="ragged reference requires an all-live width up to 8192")
                    continue
                if not admission["actual_focus_forward"]:
                    admission.update(state="cpu_reference_refused", reason="source did not observe an actual Focus forward")
                    continue
                half.validate_native(torch, config, False, row)
                started = time.monotonic()
                print(json.dumps({"event": "reference_start", "case": case["case"], "ordinal": admission["ordinal"], "width": width}), flush=True)
                if case["case"] == "TRAIN_Q111" and admission["ordinal"] == 1:
                    comparison, alignment_pin = legacy.read_pinned_json(CAMPAIGN / "compact-development-focus-widths-r1"
                            / "q111-native-context-comparison.private.json", Q111_COMPARISON, check)
                    receipt, reference_pin = legacy.read_pinned_json(CAMPAIGN / "compact-development-q111-publisher-reference-r2/execution.json",
                                                                    Q111_REFERENCE, check)
                    recorded = receipt["result"]
                    require(receipt["complete"] and comparison["exact_compiled_inputs_equal"] and comparison["actual_offered_and_scored_target_order_equal"]
                            and comparison["bindings"][1]["compiled"] == row == recorded["compiled"]
                            and comparison["bindings"][1]["roster"] == admission["roster"]
                            and comparison["bindings"][1]["choices"] == admission["offered_choices"], "Q111 reuse alignment failed")
                    match, fp32 = recorded["matching_fp16"], recorded["original_fp32"]
                    admission.update(state="reused_exact_reference", alignment=alignment_pin, reference=reference_pin)
                elif case["case"] == "TRAIN_Q543" and args.census_sha256 != ORIGINAL_PREFIX_SHA:
                    receipt, pin = legacy.read_pinned_json(CAMPAIGN / "compact-train-focus1024-reference-q543-r1/execution.json",
                            "1ca4ef30499c75dac97c8a047988e03aaaec7d8e774f7ba82c965ca721aaecd1", check)
                    match, fp32 = reuse_q543(receipt, case, admission, proof, proof_pin, runtime_pin)
                    admission.update(state="reused_exact_reference", reference=pin)
                else:
                    match = base["matching"](half, wider, model, row, check)
                    check()
                    fp32 = base["publisher_scores"](torch, publisher, row)
                    admission["state"] = "scored"
                check()
                admission.update(matching_fp16=match, original_fp32=fp32, matching_screen=bounds(match, row["option_mask"]),
                                 original_screen=bounds(fp32, row["option_mask"]),
                                 elapsed_seconds=time.monotonic() - started)
    require(not torch.cuda.is_initialized(), "CPU reference initialized CUDA")
    pins = dict(runtime["files"])
    for member in proof["reference_sources"] + [replay.file_pin(path) for path in (Path(__file__).resolve(), *SOURCES)]:
        pins[member["path"]] = member
    legacy.loaded_runtime(pins, replay.file_pin, check, [sys.modules[n] for n in sys.modules.keys() - before])
    checked_sources()
    check()
    return {"schema": "airk.private.compact-train-focus1024-independent-reference-batch/1", "batch": args.batch,
            "cohort": cohort, "rows": selected, "optional_worker_placement": placement, "reference_basis": proof_pin, "runtime": runtime_pin,
            "graph_sha256": proof["candidate"]["graph"]["sha256"], "ragged_t65_constant": True, "matching_t128_words_equal": True,
            "original_t128_max_abs": error, "native_scoring": False, "qualification": False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--output", type=Path)
    group.add_argument("--worker-deadline", type=float)
    group.add_argument("--check-only", action="store_true")
    parser.add_argument("--source-sha256")
    parser.add_argument("--cal-pid", type=int)
    parser.add_argument("--batch", required=True, type=int, choices=range(1, 6))
    parser.add_argument("--census-sha256", required=True)
    args = parser.parse_args()
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "checked Windows Python -I -B -S required")
    base, census = checked_sources()
    owner = base["owner"]
    replay = owner.module("replay_layer0.py", owner.REPLAY_SHA)
    wider = owner.module("verify_projection_candidate_t32.py", replay.WIDER_REFERENCE_SHA)
    if args.check_only:
        pin, rows = inputs(args, base, census, replay.read_only_bootstrap(), lambda: None)
        print(json.dumps({"cohort": pin["sha256"], "cases": [r["case"] for r in rows], "model_launched": False}))
    elif args.output:
        label = "q543" if args.batch == 5 and args.census_sha256 == ORIGINAL_PREFIX_SHA else f"batch{args.batch}"
        require(args.output.parent == CAMPAIGN and re.fullmatch(f"compact-train-focus1024-reference-{label}-r[1-9][0-9]*", args.output.name),
                "fresh approved per-batch output required")
        batch = (args.census_sha256, args.batch)
        if args.cal_pid is not None:
            batch += (args.cal_pid,)
        owner.run_owned(replay, wider, args.output, train_batch=batch)
    else:
        require(replay.file_pin(Path(__file__).resolve())["sha256"] == args.source_sha256
                and 0 < args.worker_deadline - time.monotonic() <= 600, "source or 600-second owner deadline changed")
        print(owner.MARKER + json.dumps(score(args, replay, wider, base, census), allow_nan=False))
