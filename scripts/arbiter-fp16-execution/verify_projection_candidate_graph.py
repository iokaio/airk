# SPDX-License-Identifier: Apache-2.0
"""Independently verify the private Compact FP32 Q/K/V MatMul graph delta."""

import re


_NAMESPACE = "/airk_compact_qkv_fp32/"
_CONTEXT_INPUTS = (
    "input_ids", "attention_mask", "position_ids",
    "decision_positions", "candidate_positions", "option_mask",
)
_PROJECTIONS = {
    f"/scorer/layers.{layer}/self_attn/{projection}_proj/MatMul": width
    for layer in range(28)
    for projection, width in (("q", 2048), ("k", 1024), ("v", 1024))
}


def require(condition, message):
    if not condition:
        raise ValueError("Compact projection graph: " + message)


def serialized(message):
    return message.SerializeToString(deterministic=True)


def without_nodes(model, onnx):
    copied = onnx.ModelProto()
    copied.CopyFrom(model)
    copied.graph.ClearField("node")
    return serialized(copied)


def fields(message):
    return {field.name for field, _ in message.ListFields()}


def require_cast(node, name, source, target, element_type, onnx):
    expected = onnx.NodeProto(name=name, op_type="Cast", input=[source], output=[target])
    expected.attribute.add(name="to", type=onnx.AttributeProto.INT, i=element_type)
    require(serialized(node) == serialized(expected), f"exact Cast {name} required")


def require_external_half(tensor, width, onnx):
    label = f"exact external half projection {tensor.name} required"
    require(fields(tensor) == {"dims", "data_type", "name", "data_location", "external_data"}
            and tuple(tensor.dims) == (2048, width)
            and tensor.data_type == onnx.TensorProto.FLOAT16
            and tensor.data_location == onnx.TensorProto.EXTERNAL, label)
    storage = {entry.key: entry.value for entry in tensor.external_data}
    require(len(tensor.external_data) == len(storage) == 3
            and set(storage) == {"location", "offset", "length"}
            and storage["location"] == "weights.bin"
            and re.fullmatch(r"0|[1-9][0-9]{0,19}", storage["offset"]) is not None
            and storage["length"] == str(2 * 2048 * width), label)


def baseline_census(model, onnx):
    graph = model.graph
    require(tuple(item.name for item in graph.input) == _CONTEXT_INPUTS
            and tuple(item.type.tensor_type.elem_type for item in graph.input)
                == (onnx.TensorProto.INT64,) * 5 + (onnx.TensorProto.BOOL,)
            and len(graph.output) == 1 and graph.output[0].name == "scores"
            and graph.output[0].type.tensor_type.elem_type == onnx.TensorProto.FLOAT,
            "baseline Context input and score declarations differ")
    require(not model.functions and not model.training_info and not graph.sparse_initializer,
            "unreviewed baseline model scope")

    initializers = {item.name: item for item in graph.initializer}
    values = set(initializers) | set(_CONTEXT_INPUTS)
    require(len(initializers) == len(graph.initializer) and len(values) == len(initializers) + len(_CONTEXT_INPUTS),
            "duplicate baseline input or initializer")
    nodes = {}
    for node in graph.node:
        require(node.name and node.name not in nodes and node.output
                and all(value in values for value in node.input if value),
                "ambiguous or unbound baseline node " + node.name)
        nodes[node.name] = node
        for name in node.output:
            require(name and name not in values, "ambiguous baseline output " + name)
            values.add(name)
    require(not any(name.startswith(_NAMESPACE) for name in (*nodes, *values))
            and not any(item.name.startswith(_NAMESPACE) for item in graph.value_info),
            "baseline already uses the private projection namespace")
    require("scores" in values
            and {name for name in nodes if re.fullmatch(r"/scorer/layers\.[0-9]+/self_attn/[qkv]_proj/MatMul", name)}
                == set(_PROJECTIONS), "baseline Compact Q/K/V projection census differs")
    return nodes, initializers


def verify_projection_candidate_graph(baseline, candidate, onnx):
    """Verify two parsed ModelProtos. The caller authenticates and holds the baseline and sidecar bytes."""
    require(isinstance(baseline, onnx.ModelProto) and isinstance(candidate, onnx.ModelProto)
            and baseline.HasField("graph") and candidate.HasField("graph"), "two complete ONNX models required")
    require(without_nodes(baseline, onnx) == without_nodes(candidate, onnx),
            "model metadata, graph declarations, or initializer storage changed")
    nodes, initializers = baseline_census(baseline, onnx)
    actual = candidate.graph.node
    require(len(actual) == len(nodes) + 3 * len(_PROJECTIONS), "exactly three added Casts per projection required")

    index = 0
    projections = set()
    for original in baseline.graph.node:
        if original.name not in _PROJECTIONS:
            require(serialized(original) == serialized(actual[index]),
                    "protected node changed or moved: " + original.name)
            index += 1
            continue

        require(fields(original) == {"input", "output", "name", "op_type"}
                and original.op_type == "MatMul" and len(original.input) == 2 and len(original.output) == 1,
                "baseline projection shape differs: " + original.name)
        activation, weight = original.input
        half_output = original.output[0]
        original_stem = "/airk_g5/weight/" + original.name
        require(activation == original_stem + "/input16" and half_output == original_stem + "/output16"
                and activation in nodes and len(nodes[activation].input) == 1
                and weight in initializers and weight not in projections,
                "baseline half operand or output boundary differs: " + original.name)
        layer = original.name.split("/self_attn/", 1)[0]
        require_cast(nodes[activation], activation, layer + "/input_layernorm/Mul_1_output_0", activation,
                     onnx.TensorProto.FLOAT16, onnx)
        require_external_half(initializers[weight], _PROJECTIONS[original.name], onnx)
        projections.add(weight)

        stem = _NAMESPACE[:-1] + original.name
        activation32, weight32, output32 = (stem + suffix for suffix in ("/input32", "/weight32", "/output32"))
        require_cast(actual[index], activation32, activation, activation32, onnx.TensorProto.FLOAT, onnx)
        require_cast(actual[index + 1], weight32, weight, weight32, onnx.TensorProto.FLOAT, onnx)
        float_projection = onnx.NodeProto()
        float_projection.CopyFrom(original)
        float_projection.input[:] = [activation32, weight32]
        float_projection.output[:] = [output32]
        require(serialized(actual[index + 2]) == serialized(float_projection),
                "exact float projection required: " + original.name)
        require_cast(actual[index + 3], stem + "/output16", output32, half_output, onnx.TensorProto.FLOAT16, onnx)
        index += 4

    require(index == len(actual), "unexpected trailing graph nodes")
    return {
        "projection_count": len(projections),
        "added_cast_count": len(actual) - len(nodes),
        "baseline_node_count": len(nodes),
        "candidate_node_count": len(actual),
    }
