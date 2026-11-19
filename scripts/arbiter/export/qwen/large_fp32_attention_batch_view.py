# SPDX-License-Identifier: Apache-2.0
"""Prepare a shape-only FP32 attention-mask Add candidate for Context Large."""

import argparse
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[4]
SOURCE_SHA256 = "f8ba5d3ea8a4269739ec5e517e61e4ec1f2f94560ea60014a4f5a9476c32fd94"
INPUTS = ("input_ids", "attention_mask", "position_ids", "decision_positions",
          "candidate_positions", "option_mask")
PREFIX = "airk_large_fp32_batch_view"
MASK = "/scorer/Where_output_0"
AXES = f"{PREFIX}_axis0"
MASK_VIEW = f"{PREFIX}_mask"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def rewrite(model, onnx):
    helper = onnx.helper
    graph = model.graph
    require({row.domain: row.version for row in model.opset_import} == {"": 18},
            "the pinned graph must use the standard opset 18")
    require(tuple(row.name for row in graph.input) == INPUTS
            and all(row.type.tensor_type.shape.dim[0].dim_value == 1 for row in graph.input)
            and [row.name for row in graph.output] == ["scores"],
            "the pinned batch-one scorer ABI differs")
    original = list(graph.node)
    require(len(original) == 9178
            and all(node.domain == "" for node in original),
            "the original unfused Large graph roster differs")
    named = {node.name: node for node in original}
    mask_writers = [node for node in original if MASK in node.output]
    require(len(mask_writers) == 1 and mask_writers[0].op_type == "Where",
            "the shared additive mask producer differs")
    require(not any(name.startswith(PREFIX) for node in original
                    for name in (node.name, *node.input, *node.output)),
            "candidate node or value name already exists")
    require(not any(tensor.name.startswith(PREFIX) for tensor in graph.initializer),
            "candidate initializer already exists")
    targets = {}
    for layer in range(36):
        start = f"/scorer/layers.{layer}/self_attn/"
        add, scale, softmax = (named[start + suffix] for suffix in ("Add_2", "Mul_8", "Softmax"))
        require((add.op_type, scale.op_type, softmax.op_type) == ("Add", "Mul", "Softmax")
                and list(add.input) == [scale.output[0], MASK]
                and len(add.output) == 1 and list(softmax.input) == [add.output[0]]
                and len(softmax.attribute) == 1
                and softmax.attribute[0].name == "axis" and softmax.attribute[0].i == -1,
                "the exact attention scale, mask Add or key-axis Softmax differs")
        targets[add.name] = layer

    axis = helper.make_tensor(AXES, onnx.TensorProto.INT64, [1], [0])
    mask_view = helper.make_node("Squeeze", [MASK, AXES], [MASK_VIEW],
                                 name=f"{PREFIX}_squeeze_mask")
    rewritten, regions = [], []
    for node in original:
        if node.name in targets:
            view = f"{PREFIX}_{targets[node.name]}"
            logits = helper.make_node("Squeeze", [node.input[0], AXES], [f"{view}_logits"],
                                      name=f"{view}_squeeze_logits")
            add = onnx.NodeProto()
            add.CopyFrom(node)
            add.input[:] = [logits.output[0], MASK_VIEW]
            add.output[:] = [f"{view}_masked"]
            restore = helper.make_node("Unsqueeze", [add.output[0], AXES], [node.output[0]],
                                       name=f"{view}_restore_batch")
            rewritten.extend((logits, add, restore))
            regions.append((mask_view, logits, add, restore))
        else:
            rewritten.append(node)
            if MASK in node.output:
                rewritten.append(mask_view)
    require(len(regions) == 36 and len(rewritten) == 9251, "incomplete Large attention rewrite")
    del graph.node[:]
    graph.node.extend(rewritten)
    graph.initializer.append(axis)
    return axis, regions


def verify_region(onnx, axis, nodes):
    """Check the actual emitted nodes against independent NumPy broadcasting."""
    import numpy as np
    from onnx.reference import ReferenceEvaluator

    helper = onnx.helper
    mask_view, logits, _, restore = nodes
    tensor = onnx.TensorProto.FLOAT
    scores_info = helper.make_tensor_value_info(logits.input[0], tensor, [1, 32, "T", "T"])
    mask_info = helper.make_tensor_value_info(mask_view.input[0], tensor, [1, 1, "T", "T"])
    result_info = helper.make_tensor_value_info(restore.output[0], tensor, [1, 32, "T", "T"])
    model = helper.make_model(helper.make_graph(nodes, "attention_mask_view",
                                               [scores_info, mask_info], [result_info], [axis]),
                              opset_imports=[helper.make_opsetid("", 18)])
    onnx.checker.check_model(model, full_check=True)
    evaluator = ReferenceEvaluator(model)
    rng = np.random.default_rng(0)
    for width in (17, 32, 128):
        scores = rng.standard_normal((1, 32, width, width), dtype=np.float32)
        keep = np.tril(np.ones((width, width), dtype=bool))
        keep[:, -1] = False
        mask = np.where(keep, np.float32(0), np.finfo(np.float32).min)[None, None]
        observed, = evaluator.run(None, {logits.input[0]: scores, mask_view.input[0]: mask})
        np.testing.assert_array_equal(observed.view(np.uint32), (scores + mask).view(np.uint32))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--onnx-site-packages", required=True, type=Path)
    args = parser.parse_args()
    source = args.source.resolve(strict=True)
    output = args.output.resolve()
    require(source.is_relative_to(ROOT / "artifacts")
            and output.is_relative_to(ROOT / "artifacts") and source.parent != output.parent,
            "use separately owned private graph input and output roots")
    require(output.parent.is_dir() and not output.exists(), "fresh candidate graph output required")
    raw = source.read_bytes()
    require(hashlib.sha256(raw).hexdigest() == SOURCE_SHA256, "original Large graph SHA differs")

    sys.path.insert(0, str(args.onnx_site_packages.resolve(strict=True)))
    import onnx

    model = onnx.load_model_from_string(raw)
    graph = model.graph
    abi = [[value.SerializeToString(deterministic=True) for value in rows]
           for rows in (graph.input, graph.output)]
    original_initializers = [row.SerializeToString(deterministic=True) for row in graph.initializer]
    axis, regions = rewrite(model, onnx)
    require(abi == [[value.SerializeToString(deterministic=True) for value in rows]
                    for rows in (graph.input, graph.output)]
            and original_initializers == [row.SerializeToString(deterministic=True)
                                          for row in graph.initializer[:-1]],
            "candidate changed the scoring ABI or original initializers")
    for region in (regions[0], regions[-1]):
        verify_region(onnx, axis, region)
    candidate = model.SerializeToString(deterministic=True)
    with output.open("xb") as stream:
        stream.write(candidate)
    print(json.dumps({
        "source_sha256": SOURCE_SHA256,
        "candidate_sha256": hashlib.sha256(candidate).hexdigest(),
        "candidate_bytes": len(candidate),
        "mask_adds": len(regions),
        "old_leading_pitch_at_8192": 32 * 8192 ** 2,
        "new_leading_pitch_at_8192": 8192 ** 2,
        "small_reference_widths": [17, 32, 128],
        "model_weights_read": False,
        "native_scoring_dispatched": False,
    }))


if __name__ == "__main__":
    main()
