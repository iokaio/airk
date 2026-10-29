# SPDX-License-Identifier: Apache-2.0
"""Compare a local AIRK qualification report with an independent CPU ONNX Runtime oracle."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model-dir", type=Path, required=True)
    parser.add_argument("--native-result", type=Path, required=True)
    parser.add_argument("--oracle-python", type=Path, help="optional isolated directory containing onnxruntime, numpy and tokenizers")
    args = parser.parse_args()
    if args.oracle_python:
        sys.path.insert(0, str(args.oracle_python.resolve()))
    import numpy as np
    import onnxruntime as ort
    import tokenizers
    spec = importlib.util.spec_from_file_location("airk_provision", Path(__file__).with_name("provision-local-model.py"))
    provision = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(provision)
    selected = json.loads((args.model_dir / "model.json").read_text(encoding="utf-8"))
    profile = next(p for p in provision.CATALOG["profiles"] if p["manifest"] == selected)
    manifest = provision.provision(args.model_dir, download=False, profile=profile)
    if args.native_result.stat().st_size > 1_048_576:
        raise ValueError("qualification report exceeds 1 MiB")
    native = json.loads(args.native_result.read_text(encoding="utf-8-sig"))
    identity = hashlib.sha256(b"onnxruntime-cpu/1.22.1/airk-int8-b1-1" +
        json.dumps(manifest, separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()
    if native["model"]["identity"] != identity:
        raise ValueError("native report identity differs from the pinned model/tokenizer/runtime specification")
    if not native["documents"] or not native["cases"] or native["total"] != len(native["cases"]):
        raise ValueError("native report must contain documents and nonempty relevance judgments")
    texts = native["documents"] + [case["query"] for case in native["cases"]]
    actual = np.asarray(native["document_vectors"] + native["query_vectors"], dtype=np.float64)
    if not 1 <= len(texts) <= 128 or actual.shape != (len(texts), manifest["dimensions"]):
        raise ValueError("qualification report dimensions are invalid")
    for index, case in enumerate(native["cases"]):
        retrieved = int(np.argmax(actual[:len(native["documents"])] @ actual[len(native["documents"]) + index]))
        if retrieved != case["retrieved"] or retrieved != case["expected"]:
            raise ValueError("native vectors do not reproduce the fixed relevance judgment")
    tokenizer = tokenizers.Tokenizer.from_file(str(args.model_dir / "tokenizer.json"))
    tokenizer.enable_truncation(max_length=manifest["max_tokens"], stride=0)
    tokenizer.no_padding()
    options = ort.SessionOptions()
    options.intra_op_num_threads = 1
    options.inter_op_num_threads = 1
    session = ort.InferenceSession(str(args.model_dir / "model.onnx"), sess_options=options, providers=["CPUExecutionProvider"])
    expected = []
    for text in texts:
        if not isinstance(text, str) or len(text.encode()) > 65_536:
            raise ValueError("qualification text exceeds byte budget")
        text = text.encode()[:16_384].decode("utf-8", errors="ignore")
        encoding = tokenizer.encode(text, add_special_tokens=True)
        length = next(n for n in (32, 64, 128, 256) if n >= len(encoding.ids))
        encoding.pad(length, pad_id=tokenizer.token_to_id("[PAD]"), pad_token="[PAD]")
        ids = np.asarray([encoding.ids], dtype=np.int64)
        mask = np.asarray([encoding.attention_mask], dtype=np.int64)
        types = np.asarray([encoding.type_ids], dtype=np.int64)
        values = session.run([manifest["output"]], {manifest["input_ids"]: ids,
            manifest["attention_mask"]: mask, manifest["token_type_ids"]: types})[0]
        pooled = (values.astype(np.float64) * mask[..., None]).sum(axis=1)[0]
        expected.append(pooled / np.linalg.norm(pooled))
    expected = np.asarray(expected)
    maximum_error = float(np.max(np.abs(expected - actual)))
    minimum_cosine = float(np.min(np.sum(expected * actual, axis=1)))
    report = {"oracle": "onnxruntime CPUExecutionProvider", "onnxruntime_version": ort.__version__,
              "tokenizers_version": tokenizers.__version__, "vectors": len(texts),
              "maximum_absolute_component_error": maximum_error, "minimum_cosine_similarity": minimum_cosine,
              "native_top1_correct": native["correct"], "native_top1_total": native["total"],
              "model_identity": native["model"]["identity"],
              "scope": "Numerical adapter equivalence and original small English retrieval fixtures; no production corpus claim"}
    print(json.dumps(report, indent=2))
    if not np.isfinite(actual).all() or maximum_error > 0.00001 or minimum_cosine < 0.99999:
        raise SystemExit("Native embedding differs from independent ONNX Runtime beyond qualification tolerance")
    if native["correct"] != native["total"]:
        raise SystemExit("Native fixture retrieval gate failed")


if __name__ == "__main__":
    main()
