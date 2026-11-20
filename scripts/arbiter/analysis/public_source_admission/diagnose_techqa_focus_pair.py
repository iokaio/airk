# SPDX-License-Identifier: Apache-2.0
"""Reproduce the original DEV_Q002 Focus pair admission using only its pinned tokenizer."""
import argparse
import hashlib
import json
from pathlib import Path
import runpy

from tokenizers import Tokenizer, __version__


ROOT = Path(__file__).resolve().parents[4]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
RUN = CAMPAIGN / "techqa-predispatch-pilot-run-r2"
PROFILE = "c8c5918be0f9e6a7d74e797250d0f8e7a32fd63e226d1deb662f44240651f632"


def bound(path, expected):
    if hashlib.sha256(path.read_bytes()).hexdigest() != expected:
        raise ValueError(f"changed retained input {path.name}")


def model_view(text):
    return text.replace("\\", "\\\\").replace("<", "\\u003c").replace(">", "\\u003e")


def diagnose():
    answer = RUN / "commands/answer.stdout"
    bound(answer, "9997a58787a9eed1e845b3d1941c499c63ad105549b0e80e377999cea9dbeaa0")
    packet = json.loads(answer.read_bytes())["structuredContent"]
    record = packet["records"][0]
    result = record["canonical_result"]
    query = record["arguments"]["query"]
    receipt = result["focus"]["receipt"]
    text = result["hits"][0]["text"]
    first = receipt["presentations"][0]
    if not (record["operation"] == "search" and query == packet["question"]
            and first["candidate"] == receipt["original_order"][0] == receipt["ranked_order"][0]
            and first["utf8_start"] == 0 and first["utf8_end"] == first["source_bytes"] == len(text.encode())
            and receipt["identity"]["profile"] == PROFILE and receipt["presented_count"] == 1
            and receipt["observed_input_tokens"] is None and receipt["scored_count"] == 0):
        raise ValueError("original presented pair changed")
    assets = RUN / "fixture/control/arbiter-v3/assets" / PROFILE
    bound(assets / "compiler.json", "2cba18a75ae105352ed9fdc94cb88812364710fcbaa6e2191a3addc524b86133")
    compiler = json.loads((assets / "compiler.json").read_bytes())
    bound(assets / "tokenizer.json", compiler["tokenizer"])
    reference = ROOT / "scripts/arbiter/export/qwen/qwen3-reranker06-experiment-r1/focus_reference.py"
    bound(reference, "d26caf475392e967da3380f4019f84e175d0096ae8003b317fbe38dd5d71cc36")
    recipe = runpy.run_path(str(reference))
    tokenizer = Tokenizer.from_file(str(assets / "tokenizer.json"))
    tokenizer.no_padding()
    tokenizer.no_truncation()

    class OriginalTokenizer:
        pad_token_id = tokenizer.token_to_id("<|endoftext|>")

        def encode(self, value, add_special_tokens=False):
            return tokenizer.encode(value, add_special_tokens=add_special_tokens).ids

    query_view, document_view = model_view(query), model_view(text)
    body = f"<Instruct>: {recipe['INSTRUCTION']}\n<Query>: {query_view}\n<Document>: {document_view}"
    adapter = OriginalTokenizer()
    counts = [len(adapter.encode(part)) for part in (recipe["PREFIX"], body, recipe["SUFFIX"])]
    try:
        recipe["compile_pair"](adapter, query_view, document_view, compiler["max_tokens"])
    except ValueError as error:
        if str(error) != f"Focus pair needs {sum(counts)} tokens; maximum={compiler['max_tokens']}; no hidden truncation":
            raise
        refusal = str(error)
    else:
        refusal = None
    return {"case": "DEV_Q002", "profile": PROFILE, "tokenizer_sha256": compiler["tokenizer"],
            "source_answer_sha256": hashlib.sha256(answer.read_bytes()).hexdigest(), "tokenizers_version": __version__,
            "query_bytes": len(query.encode()), "document_bytes": first["source_bytes"], "component_tokens": counts,
            "static_limit": compiler["max_tokens"], "pair_tokens": sum(counts), "reference_refusal": refusal,
            "model_forwards": 0, "production_observed_input_tokens": receipt["observed_input_tokens"]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    report = diagnose()
    if args.output:
        with args.output.open("x", encoding="utf-8", newline="\n") as stream:
            json.dump(report, stream, indent=2)
            stream.write("\n")
    print(json.dumps(report))
