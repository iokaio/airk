# SPDX-License-Identifier: Apache-2.0
"""Bounded ORT1.30 built-in CPU operator mechanics; no real model qualification."""
import argparse
import hashlib
import importlib.metadata
import json
from pathlib import Path
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def graph(onnx, name, nodes, inputs, outputs, initializers=()):
    model = onnx.helper.make_model(onnx.helper.make_graph(nodes, name, inputs, outputs, list(initializers)),
        opset_imports=[onnx.helper.make_opsetid("", 18), onnx.helper.make_opsetid("com.microsoft", 1)],
        producer_name="AIRK private development CPU operator probe")
    # Preserve compatibility of the envelope for the deliberate ORT1.22 control;
    # the contributed operators, rather than a newer IR, must cause its refusal.
    model.ir_version = 10
    onnx.checker.check_model(model)
    return model.SerializeToString()


def run_case(ort, name, payload, inputs, expected, output, cases, tolerance=1e-5):
    import numpy as np
    options = ort.SessionOptions()
    options.intra_op_num_threads = 1
    options.inter_op_num_threads = 1
    options.execution_mode = ort.ExecutionMode.ORT_SEQUENTIAL
    started = time.monotonic()
    session = ort.InferenceSession(payload, sess_options=options, providers=["CPUExecutionProvider"])
    actual = session.run(None, inputs)
    if len(actual) != len(expected):
        raise ValueError("unexpected output count")
    errors = []
    for result, gold in zip(actual, expected):
        if result.shape != gold.shape or not np.isfinite(result).all():
            raise ValueError(f"invalid output shape/values: {name}")
        error = float(np.max(np.abs(result.astype(np.float64) - gold.astype(np.float64))))
        if error > tolerance:
            raise ValueError(f"independent oracle mismatch: {name}, error={error}")
        errors.append(error)
    destination = output / (name + ".onnx")
    destination.write_bytes(payload)
    cases.append({"name": name, "model_sha256": hashlib.sha256(payload).hexdigest(), "model_bytes": len(payload),
                  "inputs": {key: value.tolist() for key, value in inputs.items()},
                  "expected": [value.tolist() for value in expected], "actual": [value.tolist() for value in actual],
                  "maximum_errors": errors, "tolerance": tolerance, "elapsed_seconds": time.monotonic() - started})
    return actual


def main():
    import numpy as np
    import onnx
    import onnxruntime as ort
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if not output.is_relative_to(ROOT / "artifacts") or output.exists():
        parser.error("output must be a fresh private directory under artifacts")
    if ort.__version__ != "1.30.0" or onnx.__version__ != "1.19.0":
        raise ValueError("prototype requires the separately pinned ORT1.30/ONNX1.19 development environment")
    source_bytes = Path(__file__).read_bytes()
    output.mkdir(parents=True)
    started = time.monotonic()
    vi = lambda name, shape: onnx.helper.make_tensor_value_info(name, onnx.TensorProto.FLOAT, shape)
    node = onnx.helper.make_node
    array = lambda value: np.asarray(value, dtype=np.float32)
    cases = []

    a = array([[[0.0], [-1.0], [1.0], [0.25]]])
    b = array([[[-0.5], [0.4], [-1.0], [0.3]]])
    bias, decay_scale = array([0.1]), array([-np.exp(0.2)])
    decay = (decay_scale.astype(np.float64) * np.logaddexp(0, a.astype(np.float64) + bias)).astype(np.float32)
    beta = (1 / (1 + np.exp(-b.astype(np.float64)))).astype(np.float32)
    gate = graph(onnx, "gate", [node("LinearAttentionGate", ["a", "bias", "scale", "b"], ["decay", "beta"], domain="com.microsoft")],
                 [vi("a", [1, 4, 1]), vi("bias", [1]), vi("scale", [1]), vi("b", [1, 4, 1])],
                 [vi("decay", [1, 4, 1]), vi("beta", [1, 4, 1])])
    run_case(ort, "gate", gate, {"a": a, "bias": bias, "scale": decay_scale, "b": b}, [decay, beta], output, cases)

    x = array([[[0.2, -0.3, 0.7, 0.5], [0.1, 0.8, -0.6, 0.4]]])
    weight = array([[[0.2, 0.3, 0.5]], [[-0.4, 0.7, 0.1]]])
    conv_bias, past = array([0.2, -0.1]), array([[[0.3, -0.2], [0.5, 0.6]]])
    combined = np.concatenate([past, x], axis=2)
    conv_gold = np.empty_like(x)
    for channel in range(2):
        for position in range(4):
            conv_gold[0, channel, position] = np.dot(combined[0, channel, position:position + 3], weight[channel, 0]) + conv_bias[channel]
    state_gold = combined[:, :, -2:]
    for activation in ("none", "silu"):
        conv = graph(onnx, "conv", [node("CausalConvWithState", ["x", "weight", "bias", "past"], ["y", "state"],
            domain="com.microsoft", ndim=1, activation=activation)],
            [vi("x", [1, 2, 4]), vi("weight", [2, 1, 3]), vi("bias", [2]), vi("past", [1, 2, 2])],
            [vi("y", [1, 2, 4]), vi("state", [1, 2, 2])])
        expected = conv_gold if activation == "none" else conv_gold / (1 + np.exp(-conv_gold))
        run_case(ort, "conv-" + activation, conv, {"x": x, "weight": weight, "bias": conv_bias, "past": past}, [expected, state_gold], output, cases)

    q = array([[[0.2, 0.8], [0.5, -0.5], [0.7, 0.3]]])
    k = array([[[1, 0], [0, 1], [0.6, 0.8]]])
    v = array([[[0.1, 0.2], [-0.4, 0.6], [0.9, -0.3]]])
    initial = array([[[[0.1, 0.2], [-0.2, 0.3]]]])
    state = initial[0, 0].astype(np.float64).copy()
    ys = []
    for position in range(3):
        state *= np.exp(float(decay[0, position, 0]))
        error = v[0, position].astype(np.float64) - k[0, position].astype(np.float64) @ state
        state += float(beta[0, position, 0]) * np.outer(k[0, position], error)
        ys.append(q[0, position].astype(np.float64) @ state)
    recurrent_gold = array([ys])
    final_gold = array([[state]])
    recurrent = graph(onnx, "gated-delta", [node("LinearAttention", ["q", "k", "v", "past", "decay", "beta"], ["y", "state"],
        domain="com.microsoft", q_num_heads=1, kv_num_heads=1, update_rule="gated_delta", scale=1.0)],
        [vi("q", [1, "T", 2]), vi("k", [1, "T", 2]), vi("v", [1, "T", 2]), vi("past", [1, 1, 2, 2]),
         vi("decay", [1, "T", 1]), vi("beta", [1, "T", 1])], [vi("y", [1, "T", 2]), vi("state", [1, 1, 2, 2])])
    inputs = {"q": q, "k": k, "v": v, "past": initial, "decay": decay[:, :3], "beta": beta[:, :3]}
    run_case(ort, "gated-delta-prefill", recurrent, inputs, [recurrent_gold, final_gold], output, cases)
    # Independent per-token recurrence with observed native carry, compared to
    # the same precomputed oracle; no assertion is derived from native output.
    carry = initial
    for position in range(3):
        expected_state = initial[0, 0].astype(np.float64).copy()
        for prefix in range(position + 1):
            expected_state *= np.exp(float(decay[0, prefix, 0]))
            expected_state += float(beta[0, prefix, 0]) * np.outer(k[0, prefix], v[0, prefix] - k[0, prefix] @ expected_state)
        step_inputs = {name: value[:, position:position + 1] for name, value in inputs.items() if name != "past"}
        step_inputs["past"] = carry
        _, carry = run_case(ort, f"gated-delta-step-{position}", recurrent, step_inputs,
            [recurrent_gold[:, position:position + 1], array([[expected_state]])], output, cases)

    # Tiny explicit pointer/gather mechanics, not a real trained head or score.
    hidden = array([[0.2, 0.4, -0.1], [0.7, -0.2, 0.8], [0.1, 0.6, 0.3], [-0.5, 0.2, 0.9], [0.4, 0.8, 0.2]])
    qw, kw = array([[0.2, -0.1], [0.3, 0.4], [-0.5, 0.6]]), array([[0.5, 0.2], [-0.2, 0.3], [0.1, -0.4]])
    raw = (hidden[[1, 3]].astype(np.float64) @ kw) @ (hidden[4].astype(np.float64) @ qw) / np.sqrt(2)
    probability = np.exp(raw - np.max(raw)); probability /= probability.sum()
    pointer = graph(onnx, "toy-pointer", [node("Gather", ["hidden", "decision"], ["h_decide"], axis=0),
        node("Gather", ["hidden", "options"], ["h_options"], axis=0), node("MatMul", ["h_decide", "qw"], ["query"]),
        node("MatMul", ["h_options", "kw"], ["keys"]), node("MatMul", ["keys", "query"], ["unscaled"]),
        node("Mul", ["unscaled", "scale"], ["scores"]), node("Softmax", ["scores"], ["probabilities"], axis=0)],
        [vi("hidden", [5, 3])], [vi("scores", [2]), vi("probabilities", [2])],
        [onnx.numpy_helper.from_array(qw, "qw"), onnx.numpy_helper.from_array(kw, "kw"),
         onnx.numpy_helper.from_array(np.asarray(4, dtype=np.int64), "decision"),
         onnx.numpy_helper.from_array(np.asarray([1, 3], dtype=np.int64), "options"),
         onnx.numpy_helper.from_array(array(1 / np.sqrt(2)), "scale")])
    run_case(ort, "toy-pointer", pointer, {"hidden": hidden}, [raw.astype(np.float32), probability.astype(np.float32)], output, cases)
    report = {"schema": "airk.development-ort130-operator-probe/1", "selected_cases": len(cases), "cases": cases,
        "versions": {name: importlib.metadata.version(name) for name in ("onnxruntime", "onnx", "numpy")},
        "providers": ["CPUExecutionProvider"], "threads": 1, "source_sha256": hashlib.sha256(source_bytes).hexdigest(),
        "elapsed_seconds": time.monotonic() - started,
        "scope": "Built-in CPU operator and toy pointer mechanics only. Not a complete Qwen architecture/export, trained readout, text compiler, service lifetime, resource or model qualification."}
    if Path(__file__).read_bytes() != source_bytes:
        raise ValueError("prototype source changed during the run")
    (output / "probe-source.py").write_bytes(source_bytes)
    (output / "receipt.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"selected_cases": len(cases), "max_error": max(max(case["maximum_errors"]) for case in cases), "elapsed_seconds": report["elapsed_seconds"]}))


if __name__ == "__main__":
    main()
