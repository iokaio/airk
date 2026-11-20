# SPDX-License-Identifier: Apache-2.0
"""Original publisher and matching Compact Q/K/V CPU scores for observed TRAIN admissions."""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import re
import struct
import sys
import time


HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("cpu_owner", HERE / "replay_compact_t1024_suffix.py")
owner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(owner)
require = owner.require
CAMPAIGN = owner.CAMPAIGN
COMPACT = "arbiter-context-qwen3-17b"
CAPTURE_PROFILE = "6e7520aa29076f0958353a158e9be3390caef48675188e14e53660d035c1068f"


def inputs(legacy, case, receipt_sha, ordinal, check):
    require((re.fullmatch(r"TRAIN_Q[0-9]+", case) or case == "ENGINEERING_FIXED")
            and re.fullmatch(r"[0-9a-f]{64}", receipt_sha) and 1 <= ordinal <= 8,
            "frozen development case, digest and ordinal required")
    directory = CAMPAIGN / f"compact-development-{case}-run-r1"
    leaf = f"fixture/context-cal-admissions/record-{ordinal:04}.json"
    if case == "ENGINEERING_FIXED":
        require(receipt_sha == "fca874f804c716e05ef3db6fcfad46e68b09060ff50e20a0c48ff1f4c7955f6b",
                "exact fixed engineering capture required")
        census, census_pin = legacy.read_pinned_json(directory / "capture.private.json", receipt_sha, check)
        owned, _ = legacy.read_pinned_json(directory / "owned.json",
                "300b6fc0d6e0126abc99585840e593ef11a01ac09206e407eb59ef6313026a49", check)
        require(census["schema"] == "airk.private.techqa-predispatch-capture/1" and census["case_id"] == case
                and owned["exit_code"] == 0 and owned["owned_descendants_settled"] is True
                and owned["owned_job_final_active_processes"] == 0, "engineering capture not settled")
        observed, = (s for s in census["native_observations"] if s["file"] == Path(leaf).name)
        record_sha = observed["sha256"]
    else:
        census_path = CAMPAIGN / f"compact-development-{case}-reconciled-r1/receipt.private.json"
        census, census_pin = legacy.read_pinned_json(census_path, receipt_sha, check)
        require(census["schema"] == "airk.private.compact-development-reconciled-admission/1"
                and census["split"] == "TRAIN" and census["case"] == case and census["fixture_settled"] is True
                and census["context"] == CAPTURE_PROFILE, "completed development admission census required")
        record_sha = census["source_proofs"][leaf]
    record, record_pin = legacy.read_pinned_json(directory / leaf, record_sha, check)
    require(record["schema"] == "airk.private-selected-Context-native-admission-observation/1"
            and record["ordinal_in_private_run"] == ordinal and record["actual_Focus_forward_observed"] is True,
            "actual before-forward input provenance differs")
    selected, = (item for item in record["compiler_census"] if item["model"] == COMPACT)
    compiled = selected["compiled"]
    require(selected["profile"] == CAPTURE_PROFILE and selected["NN_invocations"] == 0
            and selected["fits_remaining_allowance"] is True and selected["input_tokens"] == len(compiled["input_ids"])
            and 0 < selected["input_tokens"] <= record["request"]["remaining_input_tokens"]
            and len(compiled["candidate_positions"]) == len(record["request"]["roster"]["cards"]),
            "native input, card roster or recorded allowance differs")
    return case, census_pin, record_pin, record, compiled


def ragged_flash(torch, wider, q, k, v, mask, scale, check):
    width = len(mask)
    require(0 < width <= 8192 and mask == [1] * width and q.shape == (1,16,width,128)
            and k.shape == v.shape == (1,8,width,128), "native all-live Compact shape required")
    keys, values = k.repeat_interleave(2, dim=1).float(), v.repeat_interleave(2, dim=1).float()
    result = torch.empty_like(q)
    positions = torch.arange(width, device=q.device)
    for first in range(0, width, wider.FLASH_BLOCK):
        check()
        last = min(first+wider.FLASH_BLOCK, width)
        scores = torch.matmul(q[:, :, first:last, :].float(), keys[:, :, :last, :].transpose(-1,-2))
        allowed = positions[:last].unsqueeze(0) <= positions[first:last].unsqueeze(1)
        scores.masked_fill_(~allowed, -math.inf)
        value_tile = values[:, :, :last, :]
        pad = -last % wider.FLASH_BLOCK
        if pad:
            # Empty columns in the partial trailing Flash tile carry zero softmax mass.
            scores = torch.nn.functional.pad(scores, (0,pad), value=-math.inf)
            value_tile = torch.nn.functional.pad(value_tile, (0,0,0,pad))
        result[:, :, first:last, :] = wider.reverse_flash_result(torch, scores, value_tile, scale)
    require(torch.isfinite(result).all().item(), "nonfinite ragged Flash result")
    return result


def matching(half, wider, model, row, check):
    torch = model.torch
    original_linear = model._linear_half
    original_attention = half.causal_half_attention
    seen = []
    calls = []
    routes = set(wider.ROUTES)
    scale = half.f32(half.f32(128 ** -0.5) * math.log2(math.e))

    def linear(value, name):
        if name not in routes:
            return original_linear(value, name)
        check()
        seen.append(name)
        weight = model.weights[name]
        require(value.dtype == weight.dtype == torch.float16, "QKV operands must already be narrowed")
        return torch.nn.functional.linear(value.float(), weight.float()).half()

    def attention(torch, q, k, v, mask, *, context_flash_single_tile):
        require(not context_flash_single_tile, "unexpected single-tile mode")
        calls.append(len(mask))
        return ragged_flash(torch, wider, q, k, v, mask, scale, check)

    torch.set_float32_matmul_precision("highest")
    model._linear_half = linear
    half.causal_half_attention = attention
    try:
        scores = model.forward(row, context_flash_single_tile=False)
    finally:
        del model._linear_half
        half.causal_half_attention = original_attention
    require(tuple(seen) == wider.ROUTES and calls == [len(row["input_ids"])]*28,
            "expected exact Compact 84 QKV projections and 28 Flash calls")
    require(scores.shape == (1,len(row["candidate_positions"])) and scores.dtype == torch.float32
            and torch.isfinite(scores).all().item(), "invalid matching score vector")
    return scores[0].tolist()


def publisher_scores(torch, model, row):
    args = [torch.tensor([row[key]], dtype=torch.int64) for key in ("input_ids","attention_mask","position_ids")]
    args.extend((torch.tensor([row["decision_position"]], dtype=torch.int64),
                 torch.tensor([row["candidate_positions"]], dtype=torch.int64),
                 torch.tensor([row["option_mask"]], dtype=torch.bool)))
    scores, _hidden = model(*args)
    require(scores.shape == (1,len(row["candidate_positions"])) and scores.dtype == torch.float32
            and torch.isfinite(scores).all().item(), "invalid original FP32 scores")
    return scores[0].tolist()


def worker(args, replay, wider):
    def check():
        require(time.monotonic() < args.worker_deadline, "representative reference deadline expired")
    legacy = replay.read_only_bootstrap()
    case, census_pin, record_pin, record, row = inputs(legacy, args.case, args.census_sha256, args.ordinal, check)
    proof, proof_pin = legacy.read_pinned_json(CAMPAIGN / "compact-fp16-qkv-fp32-wider-reference-r1" /
                                              wider.WIDER_LEAF, owner.REFERENCE_SHA, check)
    runtime, runtime_pin = legacy.read_pinned_json(replay.RUNTIME, replay.RUNTIME_SHA, check)
    for pin in proof["publisher_files"].values():
        require(replay.file_pin(Path(pin["path"]), check) == pin, "publisher inputs changed")
    before = set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import torch
    torch.set_num_threads(wider.WIDER_CPU_THREADS)
    torch.set_num_interop_threads(1)
    half = owner.module("../arbiter-fp16-reference.py", legacy.REFERENCES[legacy.REFERENCE_OWNER])
    pub = owner.module("../" + legacy.PUBLISHER_OWNER.removeprefix("scripts/"), legacy.REFERENCES[legacy.PUBLISHER_OWNER])
    half.freeze_reference_math(torch)
    original, config = pub.load_authenticated(legacy.PUBLISHER_ROOT, "Qwen/Qwen3-1.7B")
    half.validate_native(torch, config, False, row)
    model = half.HalfReference(torch, config, original.text.state_dict(), False, "cpu")
    control = proof["references"][0]
    require(len(control["input"]["input_ids"]) == 128, "sealed regression input differs")
    with torch.inference_mode():
        # Every causal weighted mean of a constant vector must retain that constant,
        # including the one-live-key partial trailing tile. A padded key may carry no mass.
        q = torch.zeros((1,16,65,128), dtype=torch.float16)
        k = torch.zeros((1,8,65,128), dtype=torch.float16)
        unit = ragged_flash(torch, wider, q, k, torch.ones_like(k), [1]*65, .125, check)
        require(torch.equal(unit, torch.ones_like(unit)), "partial trailing tile assigned mass to padded keys")
        baseline = matching(half, wider, model, control["input"], check)
        require(struct.pack("<8f", *baseline) == struct.pack("<8f", *control["matching_qkv_fp32"]),
                "matching recipe changed sealed T128 score words")
        original_baseline = publisher_scores(torch, original, control["input"])
        original_control_error = max(abs(x-y) for x,y in zip(original_baseline,control["original_fp32"],strict=True))
        require(original_control_error <= .001, "original publisher control exceeds existing matching limit")
        match_scores = matching(half, wider, model, row, check)
        check()
        original_scores = publisher_scores(torch, original, row)
    check()
    require(not torch.cuda.is_initialized(), "CPU worker initialized CUDA")
    pins = dict(runtime["files"])
    for pin in proof["reference_sources"] + [replay.file_pin(Path(__file__).resolve())]:
        pins[pin["path"]] = pin
    legacy.loaded_runtime(pins, replay.file_pin, check, [sys.modules[n] for n in sys.modules.keys()-before])
    return {"schema": "airk.private.compact-observed-development-reference/1", "case": case,
            "capture_focus": record["actual_Focus_profile"], "profile": CAPTURE_PROFILE, "census": census_pin,
            "capture": record_pin, "reference_basis": proof_pin, "runtime": runtime_pin, "ordinal": args.ordinal,
            "compiled": row, "roster": record["request"]["roster"], "offered_choices": record["request"]["choices"],
            "remaining_input_tokens": record["request"]["remaining_input_tokens"],
            "matching_fp16": match_scores, "original_fp32": original_scores,
            "candidate_graph_sha256": proof["candidate"]["graph"]["sha256"],
            "ragged_constant_value_control": True, "matching_t128_score_words_equal": True, "original_t128_max_abs": original_control_error,
            "qualification": False, "native_scoring": False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--output", type=Path)
    group.add_argument("--worker-deadline", type=float)
    parser.add_argument("--source-sha256")
    parser.add_argument("--case", required=True)
    parser.add_argument("--census-sha256", required=True)
    parser.add_argument("--ordinal", type=int, required=True)
    args = parser.parse_args()
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "checked Windows Python -I -B -S required")
    replay = owner.module("replay_layer0.py", owner.REPLAY_SHA)
    wider = owner.module("verify_projection_candidate_t32.py", replay.WIDER_REFERENCE_SHA)
    if args.output:
        owner.run_owned(replay, wider, args.output, representative=(args.case,args.census_sha256,args.ordinal))
    else:
        require(replay.file_pin(Path(__file__).resolve())["sha256"] == args.source_sha256
                and 0 < args.worker_deadline-time.monotonic() <= 600, "owned source/deadline differs")
        print(owner.MARKER+json.dumps(worker(args, replay, wider), allow_nan=False))
