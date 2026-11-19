# SPDX-License-Identifier: Apache-2.0
"""Private Compact Q/K/V projection experiment over authenticated graph metadata."""


_NAMESPACE = "/airk_compact_qkv_fp32"
_BOUNDARY = "/airk_g5/weight/"
_INPUTS = (
    "input_ids", "attention_mask", "position_ids", "decision_positions",
    "candidate_positions", "option_mask",
)
_PROJECTION_WIDTHS = {"k": 1024, "v": 1024, "q": 2048}


def _require(condition, message):
    if not condition:
        raise ValueError(message)


def _cast_matches(onnx, node, input_name, output_name, dtype):
    if node is None:
        return False
    return (
        node.domain == "" and node.op_type == "Cast"
        and list(node.input) == [input_name] and list(node.output) == [output_name]
        and len(node.attribute) == 1 and node.attribute[0].name == "to"
        and node.attribute[0].type == onnx.AttributeProto.INT
        and node.attribute[0].i == dtype and not node.attribute[0].ref_attr_name
    )


def _external_half_span(onnx, tensor, width):
    types = onnx.TensorProto
    _require(tensor is not None and tensor.data_type == types.FLOAT16
             and tensor.data_location == types.EXTERNAL and tuple(tensor.dims) == (2048, width)
             and {field.name for field, _ in tensor.ListFields()} ==
             {"dims", "data_type", "name", "data_location", "external_data"},
             "expected one external Compact half projection weight")
    fields = {item.key: item.value for item in tensor.external_data}
    _require(len(tensor.external_data) == len(fields) == 3
             and set(fields) == {"location", "offset", "length"} and fields["location"] == "weights.bin"
             and fields["length"] == str(2048 * width * 2),
             "unexpected Compact projection weight sidecar range")
    offset = fields["offset"]
    _require(offset.isascii() and offset.isdecimal() and len(offset) <= 20 and offset == str(int(offset))
             and int(offset) % 2 == 0, "invalid Compact projection weight offset")
    return int(offset), int(fields["length"])


def rewrite_compact_qkv_fp32(onnx, baseline):
    """Clone metadata and widen just the exact 84 Compact Q/K/V MatMul computations.

    The caller authenticates the baseline graph and its complete external sidecar.
    This function does not open files, load tensors or alter the supplied model.
    """
    graph = baseline.graph
    types = onnx.TensorProto
    _require(baseline.HasField("graph") and not baseline.functions and not baseline.training_info
             and not graph.sparse_initializer
             and all(attr.type not in (onnx.AttributeProto.GRAPH, onnx.AttributeProto.GRAPHS)
                     for node in graph.node for attr in node.attribute),
             "expected one ordinary Compact graph")
    _require(tuple(item.name for item in graph.input) == _INPUTS
             and tuple(item.type.tensor_type.elem_type for item in graph.input) == (types.INT64,) * 5 + (types.BOOL,)
             and len(graph.output) == 1 and graph.output[0].name == "scores"
             and graph.output[0].type.tensor_type.elem_type == types.FLOAT, "Compact public graph ABI differs")

    positions = {node.name: i for i, node in enumerate(graph.node)}
    initializers = {item.name: item for item in graph.initializer}
    produced = {value: i for i, node in enumerate(graph.node) for value in node.output if value}
    _require(len(positions) == len(graph.node) and len(initializers) == len(graph.initializer)
             and len(produced) == sum(bool(value) for node in graph.node for value in node.output),
             "ambiguous baseline graph node, initializer or value")
    names = set(positions) | set(initializers) | set(produced)
    names.update(item.name for item in (*graph.input, *graph.output, *graph.value_info))
    _require(not any(name.startswith(_NAMESPACE) for name in names), "private projection namespace already present")

    roster = {
        f"/scorer/layers.{layer}/self_attn/{projection}_proj/MatMul": (layer, width)
        for layer in range(28) for projection, width in _PROJECTION_WIDTHS.items()
    }
    suffixes = tuple(f"/self_attn/{name}_proj/MatMul" for name in _PROJECTION_WIDTHS)
    _require({name for name in positions if name.endswith(suffixes)} == set(roster),
             "expected exactly the named Compact Q/K/V projection roster")

    consumers = {}
    for node in graph.node:
        for index, value in enumerate(node.input):
            consumers.setdefault(value, []).append((node.name, index))
    spans = []
    for name, (layer, width) in roster.items():
        matmul = graph.node[positions[name]]
        stem = _BOUNDARY + name
        input16, output16 = stem + "/input16", stem + "/output16"
        output_cast = stem + "/output32"
        normalized = f"/scorer/layers.{layer}/input_layernorm/Mul_1_output_0"
        _require(matmul.domain == "" and matmul.op_type == "MatMul" and not matmul.attribute
                 and len(matmul.input) == 2 and matmul.input[0] == input16 and list(matmul.output) == [output16],
                 "Compact projection MatMul boundary differs: " + name)
        _require(input16 in positions and output_cast in positions and normalized in produced
                 and produced[normalized] < positions[input16] < positions[name] < positions[output_cast],
                 "Compact projection cast topology differs: " + name)
        after = graph.node[positions[output_cast]]
        _require(_cast_matches(onnx, graph.node[positions[input16]], normalized, input16, types.FLOAT16)
                 and len(after.output) == 1 and after.output[0]
                 and _cast_matches(onnx, after, output16, after.output[0], types.FLOAT)
                 and consumers.get(input16) == [(name, 0)]
                 and consumers.get(output16) == [(output_cast, 0)],
                 "Compact projection rounding boundary differs: " + name)
        weight = matmul.input[1]
        _require(consumers.get(weight) == [(name, 1)], "aliased Compact projection weight: " + name)
        spans.append(_external_half_span(onnx, initializers.get(weight), width))

    spans.sort()
    _require(all(start + size <= following for (start, size), (following, _) in zip(spans, spans[1:])),
             "overlapping Compact projection weight ranges")
    candidate = onnx.ModelProto()
    candidate.CopyFrom(baseline)
    del candidate.graph.node[:]
    nodes = candidate.graph.node
    make_node = onnx.helper.make_node
    for node in graph.node:
        if node.name not in roster:
            nodes.add().CopyFrom(node)
            continue
        stem = _NAMESPACE + node.name
        input32, weight32 = stem + "/input32", stem + "/weight32"
        output32 = stem + "/output32"
        nodes.extend((
            make_node("Cast", [node.input[0]], [input32], name=input32, to=types.FLOAT),
            make_node("Cast", [node.input[1]], [weight32], name=weight32, to=types.FLOAT),
        ))
        widened = nodes.add()
        widened.CopyFrom(node)
        widened.input[:] = [input32, weight32]
        widened.output[:] = [output32]
        nodes.append(make_node("Cast", [output32], list(node.output), name=stem + "/output16", to=types.FLOAT16))
    return candidate
