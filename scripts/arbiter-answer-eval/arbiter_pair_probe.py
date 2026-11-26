# SPDX-License-Identifier: Apache-2.0
"""Score candidate pairs with a Qwen3 reranker checkpoint, outside AIRK.

Evaluation tooling for the single-stage Arbiter plan. It answers whether the
published checkpoint ranks the right candidate first when a decision is put
to it as a yes/no pair under a task instruction. It runs the checkpoint with
PyTorch and Transformers from a private virtual environment. It is not the
product runtime and establishes nothing about an AIRK export.

Input: a JSON file ``{"groups": [{"id", "use", "instruction", "query",
"candidates": [{"id", "text", "label"}]}]}``. ``label`` is 1 for a candidate
the evaluator holds to be right and 0 otherwise. The prompt is the
publisher's: a system line, then ``<Instruct>``, ``<Query>`` and
``<Document>``, and the score is the probability of ``yes`` against ``no`` at
the last position.

    python -B arbiter_pair_probe.py --model <dir> --pairs <file> --output <new file> [--precision fp16|4bit]

Output: every candidate's score and rank, and per use the share of groups
whose top candidate is right, the mean reciprocal rank of the best right
candidate, seconds, pairs and peak GPU memory.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys
import time

SCHEMA = "airk.private.arbiter-pair-probe/1"
PREFIX = ("<|im_start|>system\nJudge whether the Document meets the requirements based on the Query and the "
          "Instruct provided. Note that the answer can only be \"yes\" or \"no\".<|im_end|>\n<|im_start|>user\n")
SUFFIX = "<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n"


def pair_text(instruction: str, query: str, document: str) -> str:
    return f"<Instruct>: {instruction}\n<Query>: {query}\n<Document>: {document}"


def rank_summary(groups: list[dict]) -> dict:
    """Per use: groups, top-1 share and mean reciprocal rank of the best right candidate. A group with no
    right candidate, or with only right candidates, cannot rank anything and is counted as skipped."""
    by_use: dict[str, dict] = {}
    for group in groups:
        row = by_use.setdefault(group["use"], {"groups": 0, "skipped": 0, "top1": 0, "reciprocal": 0.0})
        labels = [candidate["label"] for candidate in group["candidates"]]
        if not any(labels) or all(labels):
            row["skipped"] += 1
            continue
        ordered = sorted(group["candidates"], key=lambda candidate: (-candidate["score"], candidate["id"]))
        best = next(index for index, candidate in enumerate(ordered, 1) if candidate["label"])
        row["groups"] += 1
        row["top1"] += best == 1
        row["reciprocal"] += 1.0 / best
    for row in by_use.values():
        scored = row["groups"]
        row["top1_share"] = round(row["top1"] / scored, 4) if scored else None
        row["mean_reciprocal_rank"] = round(row.pop("reciprocal") / scored, 4) if scored else None
    return by_use


def validate(document: dict) -> list[dict]:
    groups = document.get("groups")
    if not isinstance(groups, list) or not groups:
        raise ValueError("pairs file has no groups")
    seen = set()
    for group in groups:
        for key in ("id", "use", "instruction", "query"):
            if not isinstance(group.get(key), str) or not group[key]:
                raise ValueError(f"a group has no {key}")
        if group["id"] in seen:
            raise ValueError(f"duplicate group id {group['id']}")
        seen.add(group["id"])
        candidates = group.get("candidates")
        if not isinstance(candidates, list) or len(candidates) < 2:
            raise ValueError(f"group {group['id']} needs at least two candidates")
        for candidate in candidates:
            if (not isinstance(candidate.get("id"), str) or not isinstance(candidate.get("text"), str)
                    or candidate.get("label") not in (0, 1)):
                raise ValueError(f"group {group['id']} has an invalid candidate")
    return groups


def score(groups: list[dict], model_dir: pathlib.Path, precision: str, max_tokens: int, batch: int) -> dict:
    import torch
    from transformers import AutoModelForCausalLM, AutoTokenizer

    if not torch.cuda.is_available():
        raise RuntimeError("CUDA is not available; this probe does not fall back to CPU")
    tokenizer = AutoTokenizer.from_pretrained(model_dir, padding_side="left", local_files_only=True)
    options: dict = {"local_files_only": True, "device_map": {"": 0}}
    if precision == "4bit":
        from transformers import BitsAndBytesConfig
        options["quantization_config"] = BitsAndBytesConfig(
            load_in_4bit=True, bnb_4bit_quant_type="nf4", bnb_4bit_compute_dtype=torch.float16)
    else:
        options["torch_dtype"] = torch.float16
    model = AutoModelForCausalLM.from_pretrained(model_dir, **options).eval()
    yes, no = tokenizer.convert_tokens_to_ids("yes"), tokenizer.convert_tokens_to_ids("no")
    prefix = tokenizer.encode(PREFIX, add_special_tokens=False)
    suffix = tokenizer.encode(SUFFIX, add_special_tokens=False)
    room = max_tokens - len(prefix) - len(suffix)
    rows = [(group, candidate) for group in groups for candidate in group["candidates"]]
    torch.cuda.reset_peak_memory_stats()
    started, clipped = time.monotonic(), 0
    for offset in range(0, len(rows), batch):
        chunk = rows[offset:offset + batch]
        encoded = []
        for group, candidate in chunk:
            ids = tokenizer.encode(pair_text(group["instruction"], group["query"], candidate["text"]),
                                   add_special_tokens=False)
            clipped += len(ids) > room
            encoded.append({"input_ids": prefix + ids[:room] + suffix})
        inputs = tokenizer.pad(encoded, padding=True, return_tensors="pt").to(model.device)
        with torch.no_grad():
            logits = model(**inputs).logits[:, -1, :]
        pair = torch.stack([logits[:, no], logits[:, yes]], dim=1).float()
        scores = torch.nn.functional.softmax(pair, dim=1)[:, 1].tolist()
        for (_, candidate), value in zip(chunk, scores):
            candidate["score"] = round(float(value), 6)
    elapsed = time.monotonic() - started
    return {"pairs": len(rows), "clipped_pairs": clipped, "seconds": round(elapsed, 3),
            "seconds_per_pair": round(elapsed / len(rows), 4),
            "peak_gpu_bytes": int(torch.cuda.max_memory_allocated()),
            "device": torch.cuda.get_device_name(0), "torch": torch.__version__}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--model", required=True, type=pathlib.Path)
    parser.add_argument("--pairs", required=True, type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    parser.add_argument("--precision", choices=("fp16", "4bit"), default="fp16")
    parser.add_argument("--max-tokens", type=int, default=2048)
    parser.add_argument("--batch", type=int, default=8)
    args = parser.parse_args(argv)
    groups = validate(json.loads(args.pairs.read_text(encoding="utf-8")))
    run = score(groups, args.model, args.precision, args.max_tokens, args.batch)
    for group in groups:
        ordered = sorted(group["candidates"], key=lambda candidate: (-candidate["score"], candidate["id"]))
        for rank, candidate in enumerate(ordered, 1):
            candidate["rank"] = rank
            candidate.pop("text", None)
        for key in ("instruction", "query"):
            group.pop(key, None)
    result = {"schema": SCHEMA, "precision": args.precision, "max_tokens": args.max_tokens, "run": run,
              "summary": rank_summary(groups), "groups": groups}
    with args.output.open("x", encoding="utf-8", newline="\n") as handle:
        handle.write(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"run": run, "summary": result["summary"]}, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
