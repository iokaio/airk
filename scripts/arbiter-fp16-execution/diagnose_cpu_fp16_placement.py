# SPDX-License-Identifier: Apache-2.0
"""Inspect CPU placement of literal FP16-only graphs without executing inference."""

import argparse
import hashlib
import json
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[2]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
sys.path.insert(0, str(ROOT / "scripts/arbiter/evaluation/selected_goal/cal_raw"))
from r5_chronological_inputs import check_outputs
RUNTIME = CAMPAIGN / "compact-fp16-present-conversion-plan-r2/compact-runtime.json"
RUNTIME_SHA = "a557be764ebcf8a2b777403ac349f2d3ecadfabb5d65e16b8c47026239644c95"
CPU = "CPUExecutionProvider"


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def checked_runtime():
    manifest = RUNTIME.read_bytes()
    if digest(manifest) != RUNTIME_SHA:
        raise ValueError("runtime manifest SHA differs")
    runtime = json.loads(manifest)
    sys.path.extend(runtime["import_paths"])
    import onnx
    import onnxruntime
    from onnxruntime.capi import onnxruntime_pybind11_state

    for module in (onnx, onnxruntime, onnxruntime_pybind11_state):
        path = Path(module.__file__).resolve()
        if runtime["files"][str(path)]["sha256"] != digest(path.read_bytes()):
            raise ValueError(f"runtime source differs for {module.__name__}")
    return onnx, onnxruntime


def describe(onnx, ort, operation, output):
    h = onnx.helper
    half = onnx.TensorProto.FLOAT16
    x = h.make_tensor_value_info("x", half, [2, 3])
    inputs = [x]
    result_shape = [2, 3]
    if operation == "MatMul":
        inputs.append(h.make_tensor_value_info("weight", half, [3, 2]))
        result_shape = [2, 2]
    y = h.make_tensor_value_info("y", half, result_shape)
    node = h.make_node(operation, [item.name for item in inputs], ["y"])
    model = h.make_model(h.make_graph([node], operation, inputs, [y]),
                         ir_version=8, opset_imports=[h.make_opsetid("", 17)])
    options = ort.SessionOptions()
    options.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL
    options.log_severity_level = 3
    options.intra_op_num_threads = 1
    options.inter_op_num_threads = 1
    options.optimized_model_filepath = str(output)
    try:
        session = ort.InferenceSession(model.SerializeToString(), options, providers=[CPU])
    except (RuntimeError, ort.capi.onnxruntime_pybind11_state.Fail,
            ort.capi.onnxruntime_pybind11_state.NotImplemented) as error:
        return {"op": operation, "load_refused": str(error)}
    if session.get_providers() != [CPU]:
        raise ValueError("unexpected execution provider")
    check_outputs(output.parent, [output.name], created=True)
    graph = onnx.load_model(output, load_external_data=False).graph
    casts = [
        h.get_attribute_value(attr)
        for opt in graph.node if opt.op_type == "Cast"
        for attr in opt.attribute if attr.name == "to"
    ]
    return {
        "op": operation,
        "optimized_operators": [opt.op_type for opt in graph.node],
        "inserted_float32_casts": casts.count(onnx.TensorProto.FLOAT),
        "optimized_graph_sha256": digest(output.read_bytes()),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    folder = parser.parse_args().output
    if not (sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode):
        raise ValueError("use checked Windows Python -I -B -S")
    if folder.resolve(strict=True) != folder or folder.parent != CAMPAIGN or any(folder.iterdir()):
        raise ValueError("fresh guarded campaign directory required")
    check_outputs(folder, ["MatMul.onnx", "Softmax.onnx", "execution.json"], created=False)
    onnx, ort = checked_runtime()
    checks = [describe(onnx, ort, name, folder / f"{name}.onnx") for name in ("MatMul", "Softmax")]
    result = {"runtime": ort.__version__, "provider": CPU, "model_forwards": 0, "checks": checks}
    path = folder / "execution.json"
    with path.open("x", encoding="utf-8") as report:
        json.dump(result, report, allow_nan=False)
    check_outputs(folder, [path.name], created=True)
    print(json.dumps(result, allow_nan=False))


if __name__ == "__main__":
    main()
