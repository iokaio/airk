# SPDX-License-Identifier: Apache-2.0
"""Fixed held-out tokenizer cases, checked against an independent Python pipeline.

This measures tokenizer/byte-heuristic behavior, not coding-task or billed savings.
All fixture strings are original; no user conversations or prompts are collected.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

CASES = [
    ("code", "fn classify(value: Option<u32>) -> bool { value.is_some_and(|n| n > 42) }\n"),
    ("code", "export async function lookup(id: string) { return await client.get(`/items/${id}`); }\n"),
    ("code", "def normalize(values):\n    return [item.strip().casefold() for item in values]\n"),
    ("markdown", "# Requirements\nREQ-14: Requests shall complete within 250 milliseconds.\n"),
    ("markdown", "## Design decision\nKeep original document locations and report stale revisions explicitly.\n"),
    ("plain_text", "日本語の要件：索引は元の文書の場所を保持する必要があります。\n"),
    ("plain_text", "المتطلبات: يجب الحفاظ على موضع النص الأصلي في المستند.\n"),
    ("plain_text", "Révision, naïveté, Straße; café and cafe\u0301; 😀📋🔎.\n"),
    ("structured_text", '{"limit":128,"enabled":true,"values":[1,2,3],"missing":null}\n'),
    ("structured_text", "requirement,status,owner\nREQ-52,accepted,Platform\n"),
    ("plain_text", "a_b_c_____0123456789 " * 100),
    ("markdown", "The complete input must be counted even beyond a model's inference window.\n" * 200),
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--tokenizer", type=Path, required=True)
    parser.add_argument("--model-revision-sha256", required=True)
    parser.add_argument("--oracle-python", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.oracle_python:
        sys.path.insert(0,str(args.oracle_python.resolve(strict=True)))
    import tokenizers
    binary = args.binary.resolve(strict=True)
    tokenizer_bytes = args.tokenizer.read_bytes()
    oracle = tokenizers.Tokenizer.from_str(tokenizer_bytes.decode("utf-8"))
    oracle.no_padding()
    oracle.no_truncation()
    prefix, suffix = "Document evidence:\n", "\nEnd of evidence."
    results = []
    with tempfile.TemporaryDirectory(prefix="airk-tokenizer-qualification-") as temporary:
        base = Path(temporary)
        source = base/"source"
        source.mkdir()
        shutil.copyfile(args.tokenizer,base/"tokenizer.json")
        manifest = {"schema":2,"model_revision_sha256":args.model_revision_sha256,
                    "tokenizer_file":"tokenizer.json","tokenizer_sha256":hashlib.sha256(tokenizer_bytes).hexdigest(),
                    "add_special_tokens":True,"framing":{"prefix":prefix,"suffix":suffix}}
        definition = base/"calibration.json"
        definition.write_text(json.dumps(manifest),encoding="utf-8")
        for ordinal,(kind,text) in enumerate(CASES):
            # Identical representation/framing; varying payload length. These are
            # tokenizer-only pairs, not observed retrieval savings.
            before = json.dumps({"evidence":text*3},ensure_ascii=False)
            after = json.dumps({"evidence":text},ensure_ascii=False)
            (base/"before.json").write_text(before,encoding="utf-8")
            (base/"after.json").write_text(after,encoding="utf-8")
            output = subprocess.check_output([str(binary),"--root",str(source),"--data-dir",str(base/"state"),
                "usage","calibrate","--tokenizer",str(definition),"--baseline",str(base/"before.json"),
                "--returned",str(base/"after.json"),"--generation","1","--content-kind",kind],timeout=60)
            summary = json.loads(output)["summary"]
            expected = [len(oracle.encode(prefix+value+suffix,add_special_tokens=True).ids) for value in (before,after)]
            actual = [int(summary["baseline_tokens"]),int(summary["returned_tokens"])]
            if actual != expected:
                raise AssertionError(f"Tokenizer case {ordinal} disagrees: {actual} != {expected}")
            results.append({"case":ordinal,"content_kind":kind,"utf8_bytes":[len(before.encode()),len(after.encode())],
                            "exact_tokens":actual,"four_byte_heuristic":[(len(value.encode())+3)//4 for value in (before,after)],
                            "oracle_match":True})
    report = {"schema":"airk-tokenizer-qualification/1","oracle":f"Python tokenizers/{tokenizers.__version__}",
              "tokenizer_sha256":hashlib.sha256(tokenizer_bytes).hexdigest(),"model_revision_sha256":args.model_revision_sha256,
              "cases":results,"all_oracle_counts_match":True,
              "scope":"12 fixed original held-out tokenizer cases; no fitting or filtering against these results",
              "limitations":["Not representative real client sessions or verified provider chat framing",
                             "Paired payloads are synthetic tokenizer samples, not observed task or billing savings"]}
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(report,indent=2)+"\n",encoding="utf-8")
    print(f"Tokenizer qualification passed: {args.output}")


if __name__ == "__main__":
    main()
