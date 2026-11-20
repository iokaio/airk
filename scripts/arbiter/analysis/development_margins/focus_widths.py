# SPDX-License-Identifier: Apache-2.0
"""Recompile every delivered Focus presentation in the first two frozen TRAIN calls."""
import hashlib
import json
from pathlib import Path
import runpy

from tokenizers import Tokenizer, __version__


ROOT = Path(__file__).resolve().parents[4]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
CASES = {"TRAIN_Q103": "68a1fbd599630987f89e7dc167a6170472d0ca2eb3af6dc99bdde6f84807e47b",
         "TRAIN_Q111": "88a52383434baff51373af146734be6786ff80a84cb9cad6f708d6588ad2bbae"}
PROFILE = "c8c5918be0f9e6a7d74e797250d0f8e7a32fd63e226d1deb662f44240651f632"
TOKENIZER = "aeb13307a71acd8fe81861d94ad54ab689df773318809eed3cbe794b4492dae4"
REFERENCE = ROOT / "scripts/arbiter/export/qwen/qwen3-reranker06-experiment-r1/focus_reference.py"
MODEL_VIEW = ROOT / "scripts/arbiter/export/qwen/qwen3-reranker06-static-export-r1/focus_model_view.py"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def pin(path, sha):
    require(hashlib.sha256(path.read_bytes()).hexdigest() == sha, "changed input: " + path.name)


def main():
    require(__version__ == "0.21.4", "tokenizers installation differs")
    pin(REFERENCE, "d26caf475392e967da3380f4019f84e175d0096ae8003b317fbe38dd5d71cc36")
    pin(MODEL_VIEW, "15fd96ac7cf2c37a6fea8a988e5d2c5d70d6038cf87e9f61af50bd694b7983e3")
    recipe = runpy.run_path(str(REFERENCE))
    original = runpy.run_path(str(MODEL_VIEW))

    class Adapter:
        def __init__(self, tokenizer):
            self.tokenizer = tokenizer
            self.pad_token_id = tokenizer.token_to_id("<|endoftext|>")

        def encode(self, value, add_special_tokens=False):
            return self.tokenizer.encode(value, add_special_tokens=add_special_tokens).ids

    observations = []
    for case, sha in CASES.items():
        run = CAMPAIGN / ("compact-development-" + case + "-run-r1")
        path = run / "commands/answer.stdout"
        pin(path, sha)
        packet = json.loads(path.read_bytes())["structuredContent"]
        assets = run / "fixture/control/arbiter-v3/assets" / PROFILE
        pin(assets / "compiler.json", "2cba18a75ae105352ed9fdc94cb88812364710fcbaa6e2191a3addc524b86133")
        pin(assets / "tokenizer.json", TOKENIZER)
        tokenizer = Tokenizer.from_file(str(assets / "tokenizer.json"))
        tokenizer.no_truncation()
        tokenizer.no_padding()
        adapter = Adapter(tokenizer)
        outcomes = []
        recorded_searches = 0
        for record in packet["records"]:
            if record["operation"] != "search":
                continue
            recorded_searches += 1
            result = record["canonical_result"]
            observation = result["focus"]
            if observation["kind"] != "receipt":
                outcomes.append({"search": recorded_searches, "focus": observation["kind"], "presentations": []})
                continue
            receipt = observation["receipt"]
            ranked = receipt["ranked_order"]
            require(receipt["identity"]["profile"] == PROFILE and len(ranked) == len(set(ranked)) == len(result["hits"])
                    and len(receipt["presentations"]) == receipt["presented_count"], "Focus presentation roster differs")
            query = original["model_view"](record["arguments"]["query"])
            presentations = []
            for item in receipt["presentations"]:
                hit = result["hits"][ranked.index(item["candidate"])]
                raw = hit["text"].encode()
                require(not hit["truncated"] and len(raw) == item["source_bytes"]
                        and 0 <= item["utf8_start"] < item["utf8_end"] <= len(raw), "document range differs")
                document = original["model_view"](raw[item["utf8_start"]:item["utf8_end"]].decode())
                body = f"<Instruct>: {recipe['INSTRUCTION']}\n<Query>: {query}\n<Document>: {document}"
                tokens = [adapter.encode(part) for part in (recipe["PREFIX"], body, recipe["SUFFIX"])]
                total = sum(map(len, tokens))
                widths = {}
                for width in (512, 1024):
                    try:
                        pair = recipe["compile_pair"](adapter, query, document, width)
                    except ValueError as error:
                        require(total > width and str(error) == f"Focus pair needs {total} tokens; maximum={width}; no hidden truncation",
                                "unexpected publisher rejection")
                        widths[width] = {"fits": False}
                    else:
                        require(pair.observed_tokens == total and pair.padded_tokens == width
                                and tuple(pair.input_ids[-total:]) == tuple(v for group in tokens for v in group),
                                "changed static width or a hidden truncation")
                        widths[width] = {"fits": True, "charged_tokens": width, "input_ids_sha256": hashlib.sha256(
                            json.dumps(pair.input_ids, separators=(",", ":")).encode()).hexdigest()}
                presentations.append({"candidate": item["candidate"], "tokens": total, "components": list(map(len, tokens)),
                                      "widths": widths, "length_at_most_2048": total <= 2048})
            outcomes.append({"search": recorded_searches, "focus": receipt["availability"], "scored": receipt["scored_count"],
                             "original_observed_tokens": receipt["observed_input_tokens"], "presentations": presentations})
        observations.append({"case": case, "source_answer_sha256": sha, "searches": outcomes})
    print(json.dumps({"schema": "airk.private.train-focus-presentation-width-audit/1",
                      "original_focus_profile": PROFILE, "tokenizer_sha256": TOKENIZER,
                      "native_1024_observations": 0, "model_forwards": 0, "cases": observations}, sort_keys=True))


if __name__ == "__main__":
    main()
