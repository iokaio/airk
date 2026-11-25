# SPDX-License-Identifier: Apache-2.0
"""Prepare the pinned offline MiniLM FP16 weights and verify their CPU view."""
import argparse
from hashlib import sha256
import json
from pathlib import Path
import sys
from time import perf_counter

import numpy as np
import onnx
from onnx import TensorProto, helper, numpy_helper
import onnxruntime as ort


MODEL = "sentence-transformers/all-MiniLM-L6-v2"
REVISION = "10dbd2f06a8baf40ad285b037edda610c1b9a57c"
CHECKPOINT = "53aa51172d142c89d9012cce15ae4d6cc0ca6895895114379cacb4fab128d9db"
GRAPH = "947d6a395413b002fbf9d6714837a173745aa6f6f2bb3c3c486f30e03439b68b"
TOKENIZER = "be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037"
LICENSE = "9cc7fbd69f78ff19a188025976df20d5c17b0cec5ed79ad5d7e16d2094e46abd"
INPUTS = ("input_ids", "attention_mask", "token_type_ids")
OUTPUT = "last_hidden_state"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def pinned(path, digest):
    data = path.read_bytes()
    require(sha256(data).hexdigest() == digest, f"Pinned input changed: {path}")
    return data


def authenticated_inputs(source, reference):
    acquisition = (source / "acquisition.json").read_bytes()
    export = (reference / "receipt.json").read_bytes()
    rows = json.loads(acquisition)
    baseline = json.loads(export)
    require(rows["revision"] == REVISION, "Unexpected publisher revision")
    records = {row["name"]: row for row in rows["records"]}
    for name, digest in (("model.safetensors", CHECKPOINT), ("tokenizer.json", TOKENIZER)):
        data = pinned(source / name, digest)
        record = records[name]
        require(
            (record["sha256"], record["bytes"]) == (digest, len(data)),
            f"Acquisition receipt does not bind {name}",
        )
    require(
        baseline["source_model_sha256"] == CHECKPOINT and baseline["fp32_sha256"] == GRAPH,
        "Retained original export receipt does not bind the graph to the checkpoint",
    )
    return pinned(reference / "fp32.onnx", GRAPH), {
        "publisher_revision": REVISION,
        "acquisition_receipt_sha256": sha256(acquisition).hexdigest(),
        "checkpoint_sha256": CHECKPOINT,
        "original_export_receipt_sha256": sha256(export).hexdigest(),
        "original_graph_sha256": GRAPH,
    }


def convert(original):
    model = onnx.load_model_from_string(original)
    graph = model.graph
    require(
        [item.name for item in graph.input] == list(INPUTS)
        and [item.name for item in graph.output] == [OUTPUT]
        and all(item.type.tensor_type.shape.dim[0].dim_value == 1 for item in graph.input)
        and len(graph.initializer) == 101,
        "Unexpected source graph interface",
    )
    reserved = {value for node in graph.node for value in (*node.input, *node.output)}
    reserved.update(item.name for item in (*graph.input, *graph.output, *graph.initializer))
    weights = 0
    max_error = 0.0
    for index, tensor in enumerate(graph.initializer):
        require(tensor.data_type == TensorProto.FLOAT and not tensor.external_data,
                f"Unexpected initializer type or placement: {tensor.name}")
        original_name = tensor.name
        stored_name = f"airk_fp16_initializer_{index}"
        require(stored_name not in reserved, f"Duplicate graph value: {stored_name}")
        values = numpy_helper.to_array(tensor)
        half = values.astype("<f2")
        require(np.isfinite(half).all(), f"Non-finite half weights: {original_name}")
        max_error = max(max_error, float(np.max(np.abs(values - half.astype("<f4")))))
        weights += values.size
        tensor.CopyFrom(numpy_helper.from_array(half, stored_name))
        graph.node.insert(
            index, helper.make_node("Cast", [stored_name], [original_name], to=TensorProto.FLOAT)
        )
    onnx.checker.check_model(model, full_check=True)
    return model.SerializeToString(deterministic=True), int(weights), max_error


def widened_reference(original):
    # Independently encode rounded weights as FP32 in the original graph.
    model = onnx.load_model_from_string(original)
    for tensor in model.graph.initializer:
        values = numpy_helper.to_array(tensor)
        rounded = np.float32(np.float16(values))
        tensor.CopyFrom(numpy_helper.from_array(rounded, tensor.name))
    return model.SerializeToString(deterministic=True)


def cpu_proof(original, candidate):
    options = ort.SessionOptions()
    options.intra_op_num_threads = 1
    options.inter_op_num_threads = 1
    options.enable_mem_pattern = False
    options.add_session_config_entry("session.intra_op.allow_spinning", "0")
    options.add_session_config_entry("session.inter_op.allow_spinning", "0")
    # The AIRK CPU session uses Level3. Its constant folding makes the
    # FP32 arithmetic graph equal to the explicit widened reference.
    options.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL
    widened = widened_reference(original)
    stored_weights = onnx.load_model_from_string(candidate).graph.initializer
    reference_weights = onnx.load_model_from_string(widened).graph.initializer
    require(len(stored_weights) == len(reference_weights), "Weight count changed")
    for stored, reference in zip(stored_weights, reference_weights, strict=True):
        require(
            np.array_equal(
                numpy_helper.to_array(stored).astype(np.float32), numpy_helper.to_array(reference)
            ),
            "Stored initializer disagrees with explicit widening",
        )
    sessions = [
        ort.InferenceSession(graph, options, providers=["CPUExecutionProvider"])
        for graph in (original, widened, candidate)
    ]
    witnesses = []
    for width, used in ((32, 9), (64, 64), (128, 91), (256, 256)):
        ids = np.zeros((1, width), dtype=np.int64)
        words = [2023, 2003, 1037, 3231, 2005, 6251, 4126, 1012]
        ids[0, :used] = [101, *np.resize(words, used - 2), 102]
        mask = np.zeros_like(ids)
        mask[0, :used] = 1
        arguments = dict(zip(INPUTS, (ids, mask, np.zeros_like(ids)), strict=True))
        tensors = [session.run([OUTPUT], arguments)[0] for session in sessions]
        require(
            all(t.shape == (1, width, 384) and t.dtype == np.float32
                and np.isfinite(t).all() for t in tensors),
            "Output shape, finiteness or precision changed",
        )
        base, widened, stored = tensors
        require(np.array_equal(widened, stored), "Stored FP16 graph disagrees with CPU widening")
        base_vector, stored_vector = (
            tensor[0, :used].mean(axis=0, dtype=np.float64) for tensor in (base, stored)
        )
        base_vector /= np.linalg.norm(base_vector)
        stored_vector /= np.linalg.norm(stored_vector)
        witnesses.append({
            "width": width,
            "unmasked_tokens": used,
            "output_max_absolute_difference": float(np.max(np.abs(base - stored))),
            "normalized_mean_max_absolute_difference": float(
                np.max(np.abs(base_vector - stored_vector))
            ),
            "normalized_mean_cosine": float(base_vector @ stored_vector),
            "equals_explicit_fp32_widening": True,
        })
    return witnesses


def json_bytes(value):
    return (json.dumps(value, indent=2, allow_nan=False) + "\n").encode("utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("source", "reference", "license", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    require(args.output.is_dir() and not any(args.output.iterdir()), "Expected an empty output folder")
    started = perf_counter()
    original, provenance = authenticated_inputs(args.source, args.reference)
    tokenizer = pinned(args.source / "tokenizer.json", TOKENIZER)
    license_bytes = pinned(args.license, LICENSE)
    graph, weight_count, weight_error = convert(original)
    witnesses = cpu_proof(original, graph)
    manifest = {
        "schema": 2,
        "name": MODEL,
        "license": "Apache-2.0",
        "source": f"https://huggingface.co/{MODEL}",
        "revision": REVISION,
        "model_sha256": sha256(graph).hexdigest(),
        "tokenizer_sha256": TOKENIZER,
        "license_sha256": LICENSE,
        "dimensions": 384,
        "max_tokens": 256,
        "pooling": "attention-mask-mean-l2",
        **{name: name for name in INPUTS},
        "output": OUTPUT,
    }
    assets = {
        "model.onnx": graph,
        "tokenizer.json": tokenizer,
        "LICENSE": license_bytes,
        "model.json": json_bytes(manifest),
    }
    receipt = {
        "schema": "airk.indexing-plan.minilm-fp16-source/1",
        "source": provenance,
        "exporter_sha256": sha256(Path(__file__).read_bytes()).hexdigest(),
        "python": sys.version,
        "onnx": onnx.__version__,
        "numpy": np.__version__,
        "onnxruntime": ort.__version__,
        "half_weight_count": weight_count,
        "weight_rounding_max_absolute_difference": weight_error,
        "cpu_proof_cases": witnesses,
        "cpu_graph_optimization": "ORT_ENABLE_ALL",
        "batch_width": 1,
        "gpu_run": False,
        "native_product_run": False,
        "wall_seconds": perf_counter() - started,
        "assets": {
            name: {"bytes": len(data), "sha256": sha256(data).hexdigest()}
            for name, data in assets.items()
        },
    }
    for name, data in {**assets, "receipt.json": json_bytes(receipt)}.items():
        with (args.output / name).open("xb") as file:
            file.write(data)


if __name__ == "__main__":
    main()
