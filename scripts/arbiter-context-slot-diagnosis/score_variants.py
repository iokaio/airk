# SPDX-License-Identifier: Apache-2.0
"""Score permuted Context inputs with a portable graph on the ONNX Runtime CPU provider.

This is a development diagnosis. It qualifies nothing. See README.md in this folder.
"""
import argparse
from datetime import datetime, timezone
import json
import math
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from layout import pin, require, spread

SCHEMA = "airk.private.context-slot-scores/1"
VARIANTS = "airk.private.context-slot-variants/1"
RAW_PANEL = "airk.private.r8-compact-scientific-raw-panel/1"


def compiled_inputs(input_ids, candidates):
    """The six compiled inputs of unpadded tokens, as the compiler writes them for a Qwen single choice."""
    tokens = len(input_ids)
    require(tokens >= 1 and len(candidates) >= 2, "a variant needs tokens and at least two candidates")
    return {"input_ids": input_ids, "attention_mask": [1] * tokens, "position_ids": list(range(tokens)),
            "decision_positions": [tokens - 1], "candidate_positions": candidates,
            "option_mask": [True] * len(candidates)}


def feeds(described, values, np):
    """Arrays for a session's inputs, given as (name, type, rank). A batch axis is added where the graph has one."""
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


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--model", required=True, type=Path)
    parser.add_argument("--weights", action="append", default=[], type=Path,
                        help="a weight file the graph reads, recorded with its digest")
    parser.add_argument("--variants", required=True, type=Path, help="variants.json from build_variants.py")
    parser.add_argument("--reference", required=True, type=Path,
                        help="the retained raw score panel; the identity variant must reproduce it")
    parser.add_argument("--tolerance", type=float, default=0.001,
                        help="largest accepted difference between an identity score and its reference")
    parser.add_argument("--intra-threads", type=int, default=8)
    parser.add_argument("--output", required=True, type=Path, help="a fresh, empty directory")
    args = parser.parse_args()
    begun = time.perf_counter()
    started_utc = datetime.now(timezone.utc).isoformat()
    require(sys.flags.isolated and sys.dont_write_bytecode, "use Python -I -B")
    require(args.output.is_dir() and not any(args.output.iterdir()), "a fresh empty output directory is required")
    variants = json.loads(args.variants.read_bytes())
    raw = json.loads(args.reference.read_bytes())
    require(variants.get("schema") == VARIANTS and raw.get("schema") == RAW_PANEL, "unexpected input schema")
    reference = {row["ordinal"]: row for row in raw["rows"]}

    import numpy as np
    import onnxruntime as ort

    options = ort.SessionOptions()
    options.intra_op_num_threads = args.intra_threads
    opened = time.perf_counter()
    session = ort.InferenceSession(str(args.model), sess_options=options, providers=["CPUExecutionProvider"])
    load_seconds = round(time.perf_counter() - opened, 3)
    require(session.get_providers() == ["CPUExecutionProvider"], "the session is not on the CPU provider alone")
    described = [(item.name, item.type, len(item.shape)) for item in session.get_inputs()]
    receipt = {"schema": SCHEMA, "qualification": False, "provider": "cpu", "onnxruntime": ort.__version__,
               "numpy": np.__version__, "intra_threads": args.intra_threads, "tolerance": args.tolerance,
               "model": pin(args.model), "weights": [pin(path) for path in args.weights],
               "variants": pin(args.variants), "reference": pin(args.reference), "started_utc": started_utc,
               "load_seconds": load_seconds, "rows": []}
    worst = []
    with (args.output / "scores.jsonl").open("x", encoding="utf-8") as lines:
        for row in variants["rows"]:
            known = reference[row["ordinal"]]
            require(known["input_digest"] == row["input_digest"], "the reference row is another input")
            entry = {"ordinal": row["ordinal"], "variants": []}
            for variant in row["variants"]:
                inputs = feeds(described, compiled_inputs(variant["input_ids"], row["candidate_positions"]), np)
                start = time.perf_counter()
                output = session.run(["scores"], inputs)[0]
                seconds = round(time.perf_counter() - start, 4)
                scores = [float(value) for value in np.asarray(output, dtype=np.float64).reshape(-1)]
                require(len(scores) == len(row["candidate_positions"]) and all(map(math.isfinite, scores)),
                        "the graph returned missing or non-finite scores")
                scored = {"name": variant["name"], "family": variant["family"], "sources": variant["sources"],
                          "tokens": variant["tokens"], "seconds": seconds, "scores": scores}
                if variant["family"] == "identity":
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
    receipt["identity_reference_difference"] = spread(worst)
    receipt["timing"] = {"forwards": len(seconds), "forward_seconds": spread(seconds),
                         "total_seconds": round(time.perf_counter() - begun, 3)}
    receipt["finished_utc"] = datetime.now(timezone.utc).isoformat()
    receipt["complete"] = True
    with (args.output / "scores.json").open("x", encoding="utf-8") as file:
        json.dump(receipt, file, indent=1, allow_nan=False)
    print(json.dumps({"rows": len(receipt["rows"]), "identity_reference_difference":
                      receipt["identity_reference_difference"], "timing": receipt["timing"]}))


if __name__ == "__main__":
    main()
