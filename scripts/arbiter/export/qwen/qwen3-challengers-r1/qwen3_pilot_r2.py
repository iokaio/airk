# SPDX-License-Identifier: Apache-2.0
"""Owned-release-only synthetic architecture/export pilot. Never task quality."""
import argparse
import hashlib
import importlib.metadata
import json
import os
from pathlib import Path
import time
import qwen3_reference_r2 as reference


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--assets", required=True, type=Path)
    args = parser.parse_args()
    # Root creates this additive release only after independent source/resource review.
    release = json.loads(args.release.read_bytes())
    if release.get("authorized_stage") not in ("qwen3-tiny-export", "qwen3-full-reference", "qwen3-full-export"):
        raise ValueError("no reviewed architecture-stage release")
    for path, digest in release["source_sha256"].items():
        if reference.sha(path) != digest:
            raise ValueError("released source drift")
    if str(Path(__file__).resolve()) not in release["source_sha256"] or str(Path(reference.__file__).resolve()) not in release["source_sha256"]:
        raise ValueError("release omits executed source")
    output = args.output.resolve()
    if output.exists() or str(output) != release["fresh_output"]:
        raise ValueError("fresh root-selected output required")
    if os.environ.get("CUBLAS_WORKSPACE_CONFIG") != ":4096:8":
        raise ValueError("deterministic cuBLAS environment missing")
    import numpy as np
    import torch
    started = time.monotonic()
    reference.freeze_math()
    stage = release["authorized_stage"]
    model_id = release.get("model_id")
    if stage == "qwen3-tiny-export":
        if model_id is not None:
            raise ValueError("tiny release cannot acquire model weights")
        config = reference.tiny_config()
        scorer = reference.make_wrapper(config)
    else:
        scorer, config = reference.load_authenticated(args.assets, model_id)
    scorer = scorer.to("cuda").eval()
    output.mkdir()
    names = list(reference.INPUT_NAMES)
    cases = release["synthetic_shapes"]
    if not cases or any(t < 2 or t > 2048 or k < 1 or k > 8 for t, k in cases):
        raise ValueError("shape outside Context ABI")
    record = []
    feeds = []
    with torch.inference_mode():
        for length, count in cases:
            ids = (torch.arange(length, device="cuda") % (config["vocab_size"] - 1) + 1)[None, :]
            values = (ids, torch.ones_like(ids), torch.arange(length, device="cuda")[None, :],
                      torch.tensor([length - 1], device="cuda"),
                      torch.arange(3, count + 3, device="cuda")[None, :],
                      torch.ones(1, count, device="cuda", dtype=torch.bool))
            if count > 1:
                values[5][0, -1] = False
            expected_scores, hidden = scorer(*values)
            # Independent public architecture forward supplies mask construction/layer path.
            official_hidden = scorer.text(input_ids=ids, attention_mask=values[1],
                                          position_ids=values[2], use_cache=False).last_hidden_state
            official_scores = torch.nn.functional.linear(official_hidden[:, -1],
                                                          scorer.text.embed_tokens.weight)[..., values[4][0]]
            official_scores = official_scores.masked_fill(~values[5], -1e4)
            hidden_error = (hidden - official_hidden).abs().max().item()
            score_error = (expected_scores - official_scores).abs().max().item()
            if any(not torch.isfinite(value).all() for value in (hidden, expected_scores, official_hidden, official_scores)):
                raise ValueError("nonfinite reference")
            if max(hidden_error, score_error) > 1e-3:
                raise ValueError("manual path differs from official architecture")
            feed = {n: v.cpu().numpy() for n, v in zip(names, values)}
            feeds.append((feed, expected_scores.cpu().numpy(), hidden.cpu().numpy()))
            record.append({"tokens": length, "options": count,
                           "official_hidden_max_abs_error": hidden_error,
                           "official_scores_max_abs_error": score_error})
        if stage.endswith("export"):
            import onnxruntime as ort
            torch.onnx.export(scorer, tuple(torch.from_numpy(feeds[0][0][n]).to("cuda") for n in names),
                              str(output / "model.onnx"), input_names=names,
                              output_names=["scores", "hidden_states"], opset_version=18,
                              dynamo=False, do_constant_folding=True, external_data=True,
                              dynamic_axes={"input_ids": {1: "tokens"}, "attention_mask": {1: "tokens"},
                                            "position_ids": {1: "tokens"}, "candidate_positions": {1: "options"},
                                            "option_mask": {1: "options"}, "scores": {1: "options"},
                                            "hidden_states": {1: "tokens"}})
            options = ort.SessionOptions()
            options.intra_op_num_threads = options.inter_op_num_threads = 1
            options.enable_cpu_mem_arena = False
            session = ort.InferenceSession(str(output / "model.onnx"), options, providers=["CPUExecutionProvider"])
            for i, (feed, scores, hidden) in enumerate(feeds):
                actual = session.run(["scores", "hidden_states"], feed)
                errors = [float(np.max(np.abs(a - b))) for a, b in zip(actual, (scores, hidden))]
                if any(not np.isfinite(a).all() for a in actual) or max(errors) > 1e-3:
                    raise ValueError("ORT full-prefill parity failed")
                record[i]["ort_max_abs_errors"] = errors
                np.savez_compressed(output / ("case-%d.npz" % i), **feed,
                                    reference_scores=scores, reference_hidden=hidden,
                                    ort_scores=actual[0], ort_hidden=actual[1])
        else:
            for i, (feed, scores, hidden) in enumerate(feeds):
                np.savez_compressed(output / ("case-%d.npz" % i), **feed,
                                    reference_scores=scores, reference_hidden=hidden)
    torch.cuda.synchronize()
    if model_id is not None:
        reference.authenticate(args.assets, model_id)
    for path, digest in release["source_sha256"].items():
        if reference.sha(path) != digest:
            raise ValueError("executed source changed during pilot")
    receipt = {"stage": stage, "model_id": model_id, "release_sha256": reference.sha(args.release),
               "config": config, "cases": record, "elapsed_seconds": time.monotonic() - started,
               "peak_cuda_allocated": torch.cuda.max_memory_allocated(),
               "peak_cuda_reserved": torch.cuda.max_memory_reserved(),
               "versions": {n: importlib.metadata.version(n) for n in ("torch", "transformers", "onnx", "onnxruntime")},
               "artifact_sha256": {p.name: reference.sha(p) for p in output.iterdir() if p.is_file()},
               "scope": "Synthetic architecture, sparse LM-row and optional static ORT parity only; no compiler/native/action-quality/calibration/platform claim."}
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(receipt), flush=True)


if __name__ == "__main__":
    main()
