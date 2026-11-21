# SPDX-License-Identifier: Apache-2.0
"""Score built Context inputs with a portable graph on the ONNX Runtime CPU or WebGPU provider.

This is a development diagnosis. It qualifies nothing. See README.md in this folder.
"""
import argparse
from datetime import datetime, timezone
import json
import math
from pathlib import Path
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(1, str(HERE.parent / "arbiter-webgpu-probe"))
from layout import pin, require, spread
import smoke

SCHEMA = "airk.private.context-slot-scores/1"
VARIANTS = "airk.private.context-slot-variants/1"
RAW_PANEL = "airk.private.r8-compact-scientific-raw-panel/1"


def compiled_inputs(input_ids, candidates):
    """The six compiled inputs of unpadded tokens, as the compiler writes them for a Qwen single choice."""
    tokens = len(input_ids)
    require(tokens >= 1 and len(candidates) >= 2, "a variant needs tokens and at least two candidates")
    return {"input_ids": input_ids, "attention_mask": [1] * tokens, "position_ids": list(range(tokens)),
            "decision_position": tokens - 1, "candidate_positions": candidates,
            "option_mask": [True] * len(candidates)}


def feeds(described, compiled, np):
    """Arrays for a session's inputs, given as (name, type, rank). A batch axis is added where the graph has one."""
    values = {key: compiled[key] for key in ("input_ids", "attention_mask", "position_ids", "candidate_positions",
                                             "option_mask")} | {"decision_positions": [compiled["decision_position"]]}
    kinds = {"tensor(int64)": np.int64, "tensor(bool)": np.bool_}
    out = {}
    for name, kind, rank in described:
        require(name in values and kind in kinds, "unexpected graph input: " + name + " " + kind)
        array = np.asarray(values[name], dtype=kinds[kind])
        if rank == array.ndim + 1:
            array = array[None, ...]
        require(rank == array.ndim, "rank mismatch for " + name)
        out[name] = array
    require(len(out) == len(values), "the graph does not take the six expected inputs")
    return out


def difference(scores, reference):
    require(len(scores) == len(reference), "scored and reference vectors differ in length")
    return max(abs(a - b) for a, b in zip(scores, reference))


def padding_plan(provider, pad_to, pad_token_id, longest):
    """The one padded length of a run, or None for an unpadded CPU run.

    A WebGPU session keeps buffers for every input length it has seen, so a WebGPU run must pad every input to
    one length. A length that does not hold the longest input is refused before the session opens.
    """
    require(provider != "webgpu" or pad_to is not None, "a WebGPU run pads every input to one length: give --pad-to")
    require(pad_to is None or (pad_token_id is not None and pad_to >= longest),
            "padding needs --pad-token-id and a length that holds the longest input of " + str(longest) + " tokens")
    return pad_to


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--model", required=True, type=Path)
    parser.add_argument("--weights", action="append", default=[], type=Path,
                        help="a weight file the graph reads, recorded with its digest")
    parser.add_argument("--variants", required=True, type=Path, help="variants.json from build_variants.py")
    parser.add_argument("--reference", type=Path,
                        help="a retained raw score panel; the identity variant of every row must reproduce it")
    parser.add_argument("--tolerance", type=float, default=0.001,
                        help="largest accepted difference between an identity score and its reference")
    parser.add_argument("--provider", choices=("cpu", "webgpu"), default="cpu")
    parser.add_argument("--option", action="append", default=[], help="WebGPU provider option as key=value")
    parser.add_argument("--session-config", action="append", default=[], help="session config entry as key=value")
    parser.add_argument("--pad-to", type=int, help="right-pad every input to this one token count")
    parser.add_argument("--pad-token-id", type=int, help="the token id written into pad positions")
    parser.add_argument("--intra-threads", type=int, default=8, help="intra-op thread count of a CPU run")
    parser.add_argument("--output", required=True, type=Path, help="a fresh, empty directory")
    args = parser.parse_args()
    begun = time.perf_counter()
    started_utc = datetime.now(timezone.utc).isoformat()
    require(sys.flags.isolated and sys.dont_write_bytecode, "use Python -I -B")
    require(args.output.is_dir() and not any(args.output.iterdir()), "a fresh empty output directory is required")
    variants = json.loads(args.variants.read_bytes())
    require(variants.get("schema") == VARIANTS, "unexpected variants schema")
    reference = None
    if args.reference:
        raw = json.loads(args.reference.read_bytes())
        require(raw.get("schema") == RAW_PANEL, "unexpected reference schema")
        reference = {row["ordinal"]: row for row in raw["rows"]}
    gpu = args.provider == "webgpu"
    longest = max(variant["tokens"] for row in variants["rows"] for variant in row["variants"])
    pad_to = padding_plan(args.provider, args.pad_to, args.pad_token_id, longest)
    padding = smoke.padding_setup(args.model, [pad_to], args.pad_token_id) if pad_to else None
    require(gpu or not args.option, "provider options apply to WebGPU only")

    import numpy as np
    import onnxruntime as ort

    provider_options = dict(item.split("=", 1) for item in args.option)
    session_config = dict(item.split("=", 1) for item in args.session_config)
    receipt = {"schema": SCHEMA, "qualification": False, "provider": args.provider,
               "provider_options": provider_options, "session_config": session_config, "padding": padding,
               "onnxruntime": ort.__version__, "numpy": np.__version__, "tolerance": args.tolerance,
               "model": pin(args.model), "weights": [pin(path) for path in args.weights],
               "variants": pin(args.variants), "reference": pin(args.reference) if args.reference else None,
               "session_helper": pin(Path(smoke.__file__)), "started_utc": started_utc, "rows": []}
    baseline = smoke.gpu_used_mib() if gpu else None
    sampler = smoke.Sampler() if gpu else None
    if sampler:
        sampler.start()
    opened = time.perf_counter()
    session = smoke.prepare_session(ort, args.model, args.provider, provider_options, session_config,
                                    None if gpu else args.intra_threads, receipt, lambda path: pin(Path(path)))()
    receipt["load_seconds"] = round(time.perf_counter() - opened, 3)
    receipt["session_providers"] = session.get_providers()
    require(gpu or session.get_providers() == ["CPUExecutionProvider"], "the session is not on the CPU provider alone")
    after_load = smoke.gpu_used_mib() if gpu else None
    described = [(item.name, item.type, len(item.shape)) for item in session.get_inputs()]
    worst = []
    with (args.output / "scores.jsonl").open("x", encoding="utf-8") as lines:
        for row in variants["rows"]:
            known = reference[row["ordinal"]] if reference else None
            require(known is None or known["input_digest"] == row["input_digest"],
                    "the reference row is another input")
            entry = {"ordinal": row["ordinal"], "input_digest": row["input_digest"], "variants": []}
            for variant in row["variants"]:
                candidates = variant.get("candidate_positions", row["candidate_positions"])
                compiled = compiled_inputs(variant["input_ids"], candidates)
                if pad_to:
                    compiled = smoke.padded(compiled, pad_to, args.pad_token_id)
                inputs = feeds(described, compiled, np)
                start = time.perf_counter()
                output = session.run(["scores"], inputs)[0]
                seconds = round(time.perf_counter() - start, 4)
                scores = [float(value) for value in np.asarray(output, dtype=np.float64).reshape(-1)]
                require(len(scores) == len(candidates) and all(map(math.isfinite, scores)),
                        "the graph returned missing or non-finite scores")
                scored = {"name": variant["name"], "family": variant["family"], "sources": variant["sources"],
                          "tokens": variant["tokens"], "seconds": seconds, "scores": scores}
                if variant["family"] == "identity" and known is not None:
                    # An identity that does not reproduce the retained scores means the rebuild is wrong.
                    scored["reference_difference"] = difference(scores, known["scores"])
                    require(scored["reference_difference"] <= args.tolerance,
                            "the identity variant of row " + str(row["ordinal"]) + " differs from its reference by "
                            + str(scored["reference_difference"]))
                    worst.append(scored["reference_difference"])
                entry["variants"].append(scored)
                lines.write(json.dumps({"ordinal": row["ordinal"]} | scored, allow_nan=False) + "\n")
                lines.flush()
                print(json.dumps({"ordinal": row["ordinal"], "variant": variant["name"], "seconds": seconds}),
                      file=sys.stderr, flush=True)
            receipt["rows"].append(entry)
    seconds = [variant["seconds"] for row in receipt["rows"] for variant in row["variants"]]
    receipt["adapter_memory_mib"] = None
    if sampler:
        receipt["adapter_memory_mib"] = {"baseline": baseline, "after_load": after_load, "peak": sampler.stop(),
                                         "after_run": smoke.gpu_used_mib()}
    # Without a reference nothing checks the unpermuted scores: only the rebuilt tokens are known to be exact.
    receipt["identity_reference_difference"] = spread(worst) if worst else None
    receipt["timing"] = {"forwards": len(seconds), "forward_seconds": spread(seconds),
                         "total_seconds": round(time.perf_counter() - begun, 3)}
    receipt["finished_utc"] = datetime.now(timezone.utc).isoformat()
    receipt["complete"] = True
    with (args.output / "scores.json").open("x", encoding="utf-8") as file:
        json.dump(receipt, file, indent=1, allow_nan=False)
    print(json.dumps({"provider": args.provider, "rows": len(receipt["rows"]),
                      "identity_reference_difference": receipt["identity_reference_difference"],
                      "adapter_memory_mib": receipt["adapter_memory_mib"], "timing": receipt["timing"]}))


if __name__ == "__main__":
    main()
