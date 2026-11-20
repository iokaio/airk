# SPDX-License-Identifier: Apache-2.0
"""Authenticate CPU reference inputs for Large. Run with -I -B -S and retain stdout privately."""

import hashlib
import json
import math
from pathlib import Path
import runpy
import sys

ROOT = Path(__file__).resolve().parents[2]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
MODEL = "arbiter-context-qwen3-4b"
PUBLISHER = ROOT / "artifacts/advanced-split/models/qwen3-challenger-assets-r1"
OWNER = "scripts/arbiter/export/qwen/qwen3-challengers-r1/qwen3_reference_r2.py"
SOURCES = {
    OWNER: "4c6f43d0d81539cfc6bf7b6ddf5ae69273e61508d0db3d22a5e7488c60560b6b",
    "scripts/arbiter-fp16-reference.py": "06ee65a7c9cad968affb0109e1c9628dad6475c8c999116191a57119a55568cc",
    "scripts/arbiter-fp16-reference/graph.py": "698c779c16a6bc768d260669c1daf0840bc2512ea9d56e2df7660f43f6909f72",
}
PAIRS = (
    (ROOT / "artifacts/advanced-split/models/weights-v2/software-v2-independent-numeric-r1"
            "/runs-native-r1/revision-r2/assembled-large",
     "c87c4edcdec2ab5e98b41bfc186c3bb7d60791e7b9efd0aea186b731eecc3dd2",
     "bfe684ec0eb492fd093cac2bfacca0a6f45ad6c42583cba7e3a2bfdfb24e7da3"),
    (CAMPAIGN / "fp32-expanded-reference-preflight-r1/large-current-supplemental-reference",
     "020669f0c44a959a0d95798eb3cc52925513e7e549496368017e0827a398aaef",
     "a5229bafbaf43c4b7775eb72eb795864c579f8bc544faa8fc8dbf43c017a7487"),
)
DIGESTS = {
    32: "ec7b4845c37fb7cbe2a64a866f8b6cdc03479d65593cdb0a1d01102fe56fc335",
    128: "896894042a04d02773bf5a48c8b557d60156b58d503cb47728539c32c4bd2994",
    1024: "948a46156a129e61b5beadce977e6314b6e96c9570a439724813fe9592fa6357",
    8192: "ca9d34d962e308e6e53af8e14101058e8f02fbb4151bcc65eda2ad88be3de1f0",
}
NATIVE = "e701eb478bd0573812a662d4aa96b52b67fa0c577228d53d6687b000c9a772ba"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def pin(path, expected):
    with path.open("rb") as stream:
        require(hashlib.file_digest(stream, "sha256").hexdigest() == expected, "changed input: " + path.name)
    return {"path": str(path), "sha256": expected}


def metadata(path, digest):
    raw = path.read_bytes()
    require(hashlib.sha256(raw).hexdigest() == digest, "metadata digest differs: " + path.name)
    return json.loads(raw)


def reference_memory(config):
    layers, heads, kv, dim, hidden, inner, vocab = (config[key] for key in (
        "num_hidden_layers", "num_attention_heads", "num_key_value_heads",
        "head_dim", "hidden_size", "intermediate_size", "vocab_size"))
    require((layers, heads, kv, dim, hidden, inner, vocab) == (36, 32, 8, 128, 2560, 9728, 151936),
            "Large geometry differs")
    protected = hidden + layers * (2 * hidden + 2 * dim)
    learned = vocab * hidden + layers * (2 * hidden * dim * (heads + kv) + 3 * hidden * inner)
    return {
        "publisher_fp32_weight_bytes": 4 * (protected + learned),
        "matching_reference_weight_bytes": 4 * protected + 2 * learned,
        "fp32_and_matching_weight_bytes_before_shared_protected_storage": 8 * protected + 6 * learned,
        "largest_8192_attention_score_tile_bytes": 4 * heads * 64 * 8192,
    }


def selected_rows(pairs, native):
    rows = {}
    for fixture, producer, producer_pin in pairs:
        require(fixture["model"] == producer["model"] == MODEL
                and fixture["schema"] == "airk.arbiter.native-numeric/2"
                and producer["schema"] == "airk.arbiter.independent-native-numeric-reference/2"
                and fixture["reference_producer"] == producer_pin
                and fixture["compiler"] == producer["compiler"]
                and fixture["resource_policy"] == producer["resource_policy_digest"]
                and fixture["original_reference_assets"] == producer["original_reference_assets"]
                and fixture["rows"] == producer["rows"], "Large fixture/producer lineage differs")
        for row in fixture["rows"]:
            source = row["input"]
            width = len(source["input_ids"])
            if width == 17:
                continue
            require(width in DIGESTS and width not in rows and source["input_digest"] == DIGESTS[width]
                    and source["attention_mask"] == [1] * width and source["position_ids"] == list(range(width))
                    and source["decision_position"] == width - 1 and len(source["candidate_positions"]) == 8
                    and source["option_mask"] == [True] * 8 and len(row["reference_original_fp32"]) == 8
                    and all(type(s) in (float, int) and math.isfinite(s) for s in row["reference_original_fp32"]),
                    "Large native input, mask, decision, or original scores differ")
            rows[width] = row
    require(sorted(rows) == list(DIGESTS) and len(native["rows"]) == 4, "complete four-width native rows required")
    for row, width in zip(native["rows"], DIGESTS, strict=True):
        require((row["tokens"], row["input_digest"], row["name"]) ==
                (width, DIGESTS[width], rows[width]["name"]), "Large CUDA FP32 input differs")
    return rows


def prepare():
    sources = [pin(ROOT / path, digest) for path, digest in SOURCES.items()]
    pairs = []
    for root, fixture_sha, producer_sha in PAIRS:
        producer = root / "reference-producer.json"
        pairs.append((metadata(root / "numeric-fixture.json", fixture_sha), metadata(producer, producer_sha),
                      {"path": str(producer), "sha256": producer_sha}))
    first, second = pairs
    require(all(first[0][key] == second[0][key] for key in ("compiler", "resource_policy", "original_reference_assets"))
            and first[1]["original_source_files"] == second[1]["original_source_files"], "mixed Large release lineage")
    native = metadata(CAMPAIGN / "large-fp32-batch-view-scoring-r1/native-result.json", NATIVE)
    rows = selected_rows(pairs, native)
    owner = runpy.run_path(str(ROOT / OWNER))
    folder, config, index = owner["authenticate"](PUBLISHER, "Qwen/Qwen3-4B")
    files = first[1]["original_source_files"]
    require(set(files) == {"config.json", "model.safetensors.index.json", "tokenizer.json", *index.values()},
            "publisher file roster differs")
    for name, file in files.items():
        path = folder / name
        require(file["path"] == str(path) and path.stat().st_size == file["bytes"], "publisher input path differs")
        pin(path, file["sha256"])
    return {
        "schema": "airk.private.large-fp16-independent-reference-inputs/1",
        "model": MODEL, "publisher_revision": owner["REVISIONS"]["Qwen/Qwen3-4B"],
        "publisher_files": files, "sources": sources, "fp32_native_sha256": NATIVE,
        "source_pairs": [{"fixture_sha256": f, "producer_sha256": p} for _, f, p in PAIRS],
        "rows": [rows[width] for width in DIGESTS], "memory_components": reference_memory(config),
        "cpu_reference_job_limit_bytes": 32 << 30, "cpu_reference_wall_seconds": 600,
        "peak_memory_measured": False, "candidate_graph_verified": False, "matching_forwards": 0,
    }


if __name__ == "__main__":
    require(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode, "run with -I -B -S")
    print(json.dumps(prepare(), sort_keys=True, allow_nan=False))
