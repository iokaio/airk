# SPDX-License-Identifier: Apache-2.0
"""Exercise candidate-config GDN CPU shapes, not complete architectures/weights."""
import argparse
import hashlib
import importlib.metadata
import json
from pathlib import Path
import time

import ort130_probe as support


def main():
    import numpy as np
    import onnx
    import onnxruntime as ort
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists() or not output.is_relative_to(support.ROOT / "artifacts"):
        parser.error("fresh private output required")
    if ort.__version__ != "1.30.0":
        raise ValueError("isolated ORT1.30 environment required")
    runner = Path(__file__).read_bytes()
    helper = Path(support.__file__).read_bytes()
    output.mkdir(parents=True)
    started = time.monotonic()
    configs = []
    for name in ("internlm--Intern-Decision-0.8B", "internlm--Intern-Decision-4B", "Qwen--Qwen3.5-2B-Base", "Qwen--Qwen3.5-4B-Base"):
        path = support.ROOT / "artifacts/advanced-split/models/publisher" / name / "config.json"
        raw = path.read_bytes()
        configs.append((name, path, raw, json.loads(raw)["text_config"]))
    cases = []
    for index, (name, path, raw, config) in enumerate(configs):
        qh, vh = config["linear_num_key_heads"], config["linear_num_value_heads"]
        dk, dv = config["linear_key_head_dim"], config["linear_value_head_dim"]
        batch, length = 2, 5
        rng = np.random.default_rng(9700 + index)
        q = rng.normal(0, .1, (batch, length, qh, dk)).astype(np.float32)
        k = rng.normal(0, .1, (batch, length, qh, dk)).astype(np.float32)
        v = rng.normal(0, .1, (batch, length, vh, dv)).astype(np.float32)
        decay = -rng.uniform(.1, .8, (batch, length, vh)).astype(np.float32)
        beta = rng.uniform(.1, .8, (batch, length, vh)).astype(np.float32)
        initial = rng.normal(0, .01, (batch, vh, dk, dv)).astype(np.float32)
        state = initial.astype(np.float64)
        expected = np.empty((batch, length, vh, dv), dtype=np.float64)
        # Independent scalar head/time iteration, sharing Q/K across contiguous
        # value-head groups, as the pinned builder's q/kv head attrs specify.
        for b in range(batch):
            for t in range(length):
                for h in range(vh):
                    kh = h // (vh // qh)
                    state[b, h] *= np.exp(float(decay[b, t, h]))
                    error = v[b, t, h].astype(np.float64) - k[b, t, kh].astype(np.float64) @ state[b, h]
                    state[b, h] += float(beta[b, t, h]) * np.outer(k[b, t, kh], error)
                    expected[b, t, h] = q[b, t, kh].astype(np.float64) @ state[b, h]
        vi = lambda key, shape: onnx.helper.make_tensor_value_info(key, onnx.TensorProto.FLOAT, shape)
        node = onnx.helper.make_node("LinearAttention", ["q", "k", "v", "past", "decay", "beta"], ["y", "state"],
            domain="com.microsoft", q_num_heads=qh, kv_num_heads=vh, update_rule="gated_delta", scale=1.)
        payload = support.graph(onnx, name, [node], [vi("q", [batch,length,qh*dk]), vi("k", [batch,length,qh*dk]),
            vi("v", [batch,length,vh*dv]), vi("past", [batch,vh,dk,dv]), vi("decay", [batch,length,vh]),
            vi("beta", [batch,length,vh])], [vi("y", [batch,length,vh*dv]), vi("state", [batch,vh,dk,dv])])
        inputs = {"q": q.reshape(batch,length,-1), "k": k.reshape(batch,length,-1), "v": v.reshape(batch,length,-1),
            "past": initial, "decay": decay, "beta": beta}
        options = ort.SessionOptions(); options.intra_op_num_threads = 1; options.inter_op_num_threads = 1
        session = ort.InferenceSession(payload, options, providers=["CPUExecutionProvider"])
        actual_y, actual_state = session.run(None, inputs)
        errors = [float(np.max(np.abs(actual_y.reshape(expected.shape).astype(np.float64) - expected))),
                  float(np.max(np.abs(actual_state.astype(np.float64) - state)))]
        if not all(np.isfinite(errors)) or max(errors) > 1e-5:
            raise ValueError(f"independent head-group oracle mismatch: {name}: {errors}")
        stem = "case-" + str(index)
        (output / (stem + ".onnx")).write_bytes(payload)
        arrays = output / (stem + ".npz")
        np.savez_compressed(arrays, **inputs, expected_y=expected, expected_state=state,
                            actual_y=actual_y, actual_state=actual_state)
        cases.append({"candidate_config": name, "config_sha256": hashlib.sha256(raw).hexdigest(),
            "batch": batch, "sequence": length, "key_heads": qh, "value_heads": vh, "key_dimension": dk,
            "value_dimension": dv, "dtype": "float32", "maximum_errors": errors,
            "arrays_sha256": hashlib.sha256(arrays.read_bytes()).hexdigest(),
            "graph_sha256": hashlib.sha256(payload).hexdigest()})
    for path, raw in [(Path(__file__),runner), (Path(support.__file__),helper)] + [(p,r) for _,p,r,_ in configs]:
        if path.read_bytes() != raw:
            raise ValueError("consumed source/config drift")
    (output / "runner.py").write_bytes(runner)
    (output / "helper.py").write_bytes(helper)
    result = {"selected_cases": len(cases), "cases": cases, "elapsed_seconds": time.monotonic()-started,
        "runner_sha256": hashlib.sha256(runner).hexdigest(), "helper_sha256": hashlib.sha256(helper).hexdigest(),
        "versions": {n:importlib.metadata.version(n) for n in ("onnxruntime","onnx","numpy")},
        "scope": "Candidate-config FP32 GDN recurrence shapes only; no complete architecture, trained weights/head, BF16/FP16 recurrence, export or native product qualification."}
    (output / "receipt.json").write_text(json.dumps(result, indent=2)+"\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
