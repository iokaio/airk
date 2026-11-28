# SPDX-License-Identifier: Apache-2.0
"""Authenticate the local file-reranker checkpoint, loaded scorer and joined control."""

import hashlib
import json
import math
from pathlib import Path

import arbiter_loop_prototype as prototype
from arbiter_pair_probe import PREFIX, SUFFIX, pair_text

REVISION = "22e683669bc0f0bd69640a1354a6d0aebcfeede5"
# Publisher Git blobs for the configuration/tokenizer; LFS SHA-256 for large files.
BLOBS = {
    "config.json": "b0d564fb244100a8bbcb6499da05485440bae0d2",
    "generation_config.json": "e4f1d3193e99a3e5d7047edf56e93a6e933fa31b",
    "tokenizer_config.json": "7345216a0785dc7086e8c245b2a9d3896ce2b756",
    "special_tokens_map.json": "ac23c0aaa2434523c494330aeb79c58395378103",
    "vocab.json": "4783fe10ac3adce15ac8f358ef5462739852c569",
    "merges.txt": "31349551d90c7606f325fe0f11bbb8bd5fa0d7c7",
    "chat_template.jinja": "63b97f268a9ddb9c6b34e6d7d8ef531d1fee9cf4",
    "model.safetensors.index.json": "c8dc285661c390e68d4864b34cbe7ec6f755b776",
    "tokenizer.json": "aeb13307a71acd8fe81861d94ad54ab689df773318809eed3cbe794b4492dae4",
    "model-00001-of-00002.safetensors": "cf2e87cbf71fa628961532232e04dd6c19702a0a057f5e2aff95ea1aca4fd488",
    "model-00002-of-00002.safetensors": "78946d22b7f6456ea7a5358dbdf3982de36c5bac1f166a5fd58e18e31db8048a",
}
OTHER_FILES = {".gitattributes", "config_sentence_transformers.json", "modules.json", "sentence_bert_config.json", "README.md"}
PREFIX_SHA = "b09144c543bfe3e0dcde88b2bf54986c50060c9f31d9fe98916e1b7f1ad3b80b"
SUFFIX_SHA = "5ca215121bd0820099042d7d3949d5b77533e17ad13bb8e6104d37029e7ea54d"
CENSUS = {"layers": 252, "quantized_parameters": 3633315840, "fp16_parameters": 388468736,
          "packed_bytes_estimate": 4864417792}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def authenticate(checkpoint):
    checkpoint = Path(checkpoint).resolve(strict=True)
    files = {p.name for p in checkpoint.iterdir() if p.name not in (".cache", "1_LogitScore")}
    require(set(BLOBS) <= files <= (set(BLOBS) | OTHER_FILES), "unexpected checkpoint contents or alternative weights")
    for name, expected in BLOBS.items():
        path = checkpoint / name
        if len(expected) == 40:
            raw = path.read_bytes()
            actual = hashlib.sha1(f"blob {len(raw)}\0".encode() + raw).hexdigest()
        else:
            sha = hashlib.sha256()
            with path.open("rb") as stream:
                for chunk in iter(lambda: stream.read(4 * 1024 * 1024), b""):
                    sha.update(chunk)
            actual = sha.hexdigest()
        require(actual == expected, f"checkpoint file differs from pinned Qwen3-Reranker-4B revision: {name}")
    return checkpoint


def inspect_loaded(judge, checkpoint):
    require(type(judge) is prototype.Judge and judge.score.__func__ is prototype.Judge.score,
            "research scoring requires the concrete local Qwen Judge")
    require(judge.weights == CENSUS, "unexpected rounded Q8 block-32 weight census")
    model = judge.model
    require(type(model).__name__ == "Qwen3ForCausalLM" and
            Path(model.config._name_or_path).resolve() == checkpoint and
            Path(judge.tokenizer.name_or_path).resolve() == checkpoint, "different loaded model or tokenizer")
    require(not model.training and all(p.device.type == "cuda" and p.dtype == judge.torch.float16
                                       for p in model.parameters()), "model must run in CUDA FP16 evaluation mode")
    require(judge.batch == 8 and judge.room + len(judge.prefix) + len(judge.suffix) == 1024 and
            judge.tokenizer.padding_side == "left" and
            judge.prefix == judge.tokenizer.encode(PREFIX, add_special_tokens=False) and
            judge.suffix == judge.tokenizer.encode(SUFFIX, add_special_tokens=False) and
            judge.yes != judge.no and judge.tokenizer.convert_ids_to_tokens([judge.yes, judge.no]) == ["yes", "no"],
            "loaded scorer differs from the frozen yes/no prompt and batching configuration")


def load_verified_judge(checkpoint):
    require((digest(PREFIX), digest(SUFFIX)) == (PREFIX_SHA, SUFFIX_SHA) and
            pair_text("I", "Q", "D") == "<Instruct>: I\n<Query>: Q\n<Document>: D", "frozen pair wrapper changed")
    checkpoint = authenticate(checkpoint)
    judge = prototype.Judge(checkpoint, max_tokens=1024, batch=8, bits=8)
    inspect_loaded(judge, checkpoint)
    return judge, {"model": "Qwen/Qwen3-Reranker-4B", "revision": REVISION, "checkpoint": str(checkpoint),
                   "publisher_blobs": dict(BLOBS), "weight_bits": 8, "block_size": 32, "clip_search": False,
                   "compute_dtype": "float16", "weights": dict(judge.weights), "batch": 8, "max_tokens": 1024,
                   "torch": str(judge.torch.__version__), "device": judge.torch.cuda.get_device_name(judge.model.device),
                   "system_prefix_sha256": PREFIX_SHA, "assistant_suffix_sha256": SUFFIX_SHA,
                   "yes_token_id": judge.yes, "no_token_id": judge.no}


def verify_control(paths, pairs, scores, order, legacy_pairs, legacy_scores=None):
    paths, scores = list(paths), list(scores)
    require(0 < len(paths) == len(set(paths)) == len(scores) == len(pairs) <= 40 and
            all(math.isfinite(s) and 0 <= s <= 1 for s in scores), "invalid joined control pool or scores")
    require(list(pairs) == list(legacy_pairs), "joined control inputs differ from the legacy query/cards")
    expected = [paths[i] for i in sorted(range(len(paths)), key=lambda i: (-scores[i], i))]
    require(list(order) == expected, "joined control gave an incorrect or unstable ranking")
    if legacy_scores is not None:
        require([r[0] for r in legacy_scores] == paths, "legacy candidate order differs")
        historic = [row[0] for row in sorted(legacy_scores, key=lambda row: -row[1])]
        require(historic == expected, "joined control rank differs from the historical score receipt")
    return {"pool": len(paths), "joined_pairs_sha256": digest(json.dumps(list(pairs), ensure_ascii=False)),
            "rank_sha256": digest(json.dumps(expected, ensure_ascii=False)), "historical_rank_checked": legacy_scores is not None}
