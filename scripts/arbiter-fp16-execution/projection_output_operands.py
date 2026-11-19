# SPDX-License-Identifier: Apache-2.0
"""Load the retained R4 Compact attention tile and unchanged half O weights.

The caller supplies the reviewed ONNX runtime and canonical exact file pins.
The two outputs are little-endian row-major binary16 bytes for [32,2048]
and [2048,2048]. The R4 evidence supplies the activation, with no observed O
output for this isolated GEMM comparison.
"""

import hashlib
import json
import math
from pathlib import Path
import struct
from types import ModuleType


CAPTURE_SHA = "9705db75efa3e84fc955c3edeaf03e1aef70e5362161087f24250e14b75aeac6"
CUSTODY_SHA = "5209c0e7282ef74903f8c68e9d00934d8f54d5f9887c14c5b74824719197bcb5"
RAW = "/airk_g5/attention/0/raw16"
RESTORE = "/airk_g5/attention/0/output32"
RESTORED = "/scorer/layers.0/self_attn/Reshape_7_output_0"
ATTENTION = "/airk_g5/attention/0/efficient"
MATMUL = "/scorer/layers.0/self_attn/o_proj/MatMul"
INPUT16 = "/airk_g5/weight/" + MATMUL + "/input16"
OUTPUT16 = "/airk_g5/weight/" + MATMUL + "/output16"
WEIGHT = "onnx::MatMul_8298"
SPAN = (639_602_688, 8_388_608)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def custody_helper():
    path = Path(__file__).with_name("projection_gemm_inputs.py")
    with path.open("rb") as source:
        raw = source.read((64 << 10) + 1)
    require(0 < len(raw) <= 64 << 10 and hashlib.sha256(raw).hexdigest() == CUSTODY_SHA,
            "reviewed Compact projection custody source differs")
    helper = ModuleType("compact_output_projection_custody")
    exec(compile(raw, str(path), "exec"), vars(helper))
    return helper


def capture_activation(capture, custody):
    require(capture["schema"] == "airk.private.compact-fp16-layer0-taps/1"
            and capture["complete"] is capture["native_forward_returned"] is capture["forward_synchronized"] is True
            and capture["qualified"] is capture["product_admission"] is False and capture.get("error") is None
            and type(capture["numeric_forward_attempts"]) is int and capture["numeric_forward_attempts"] == 1
            and capture["source_mask"]["verified_candidate_graph"] == custody.GRAPH_SHA
            and capture["source_mask"]["mask_values_require_all_live_before_dispatch"] is True,
            "expected the exact synchronized R4 Compact attention capture")
    row, = capture["rows"]
    values = row["raw_scores"]
    require(row["name"] == "diagnostic-all-live-32" and type(row["tokens"]) is int and row["tokens"] == 32
            and row["input_digest"] == custody.INPUT_DIGEST and len(values) == 8
            and all(type(value) in (int, float) and math.isfinite(value) for value in values)
            and hashlib.sha256(struct.pack("<8f", *values)).hexdigest() == custody.RAW_SCORES_SHA,
            "frozen R4 input or sealed score bits differ")
    ids = (
        ("/airk_g5/attention/0/q16", 2048),
        ("/airk_g5/attention/0/k16", 1024),
        ("/airk_g5/weight//scorer/layers.0/self_attn/v_proj/MatMul/output16", 1024),
        (RAW, 2048),
    )
    taps = capture["layer0_taps"]
    require(isinstance(taps, list) and len(taps) == len(ids)
            and all(set(tap) == {"name", "width", "bits"} and type(tap["width"]) is int
                    and (tap["name"], tap["width"]) == expected for tap, expected in zip(taps, ids)),
            "exact R4 attention tap roster differs")
    words = taps[-1]["bits"]
    require(isinstance(words, list) and len(words) == 32 * 2048
            and all(type(word) is int and 0 <= word <= 0xffff and (word & 0x7c00) != 0x7c00 for word in words),
            "expected complete finite R4 raw [1,32,2048] half values")
    return struct.pack("<65536H", *words)


def cast_matches(onnx, node, source, target, dtype):
    return (node.op_type == "Cast" and node.domain == ""
            and list(node.input) == [source] and list(node.output) == [target]
            and len(node.attribute) == 1 and node.attribute[0].name == "to"
            and node.attribute[0].type == onnx.AttributeProto.INT
            and node.attribute[0].i == dtype and not node.attribute[0].ref_attr_name)


def output_weight_range(onnx, graph, custody):
    positions = {node.name: index for index, node in enumerate(graph.node)}
    tensors = {tensor.name: tensor for tensor in graph.initializer}
    produced = {value for node in graph.node for value in node.output if value}
    require(len(positions) == len(graph.node) and len(tensors) == len(graph.initializer)
            and len(produced) == sum(bool(value) for node in graph.node for value in node.output),
            "ambiguous baseline operand value or producer")
    order = (ATTENTION, RESTORE, INPUT16, MATMUL)
    require(set(order) <= positions.keys() and [positions[name] for name in order] ==
            sorted(positions[name] for name in order), "layer-zero O activation topology differs")
    gqa, restore, narrow, projection = (graph.node[positions[name]] for name in order)
    types = onnx.TensorProto
    require(gqa.op_type == "GroupQueryAttention" and gqa.domain == "com.microsoft"
            and list(gqa.input[:3]) == ["/airk_g5/attention/0/" + name for name in ("q16", "k16", "v16")]
            and list(gqa.output) == [RAW, "/airk_g5/attention/0/present_key", "/airk_g5/attention/0/present_value"]
            and cast_matches(onnx, restore, RAW, RESTORED, types.FLOAT)
            and cast_matches(onnx, narrow, RESTORED, INPUT16, types.FLOAT16),
            "R4 raw attention does not feed O through the exact half-float-half route")
    require(projection.op_type == "MatMul" and projection.domain == "" and not projection.attribute
            and list(projection.input) == [INPUT16, WEIGHT] and list(projection.output) == [OUTPUT16]
            and [(node.name, i) for node in graph.node for i, value in enumerate(node.input) if value == WEIGHT]
            == [(MATMUL, 1)], "baseline learned half O MatMul route differs")
    weight = tensors.get(WEIGHT)
    require(weight is not None and weight.data_location == types.EXTERNAL and weight.data_type == types.FLOAT16
            and tuple(weight.dims) == (2048, 2048) and {field.name for field, _ in weight.ListFields()}
            == {"dims", "data_type", "name", "data_location", "external_data"},
            "expected a single external FP16 [2048,2048] O weight")
    spans = {tensor.name: custody.external_range(tensor, custody.WEIGHTS_SIZE)
             for tensor in graph.initializer if tensor.data_location == types.EXTERNAL}
    intervals = sorted(spans.values())
    require(spans[WEIGHT] == SPAN and all(offset + length <= next_offset
                for (offset, length), (next_offset, _) in zip(intervals, intervals[1:])),
            "O weight has a different or overlapping sidecar range")
    return SPAN


def load_output_operands(onnx, capture_path, capture_sha, model_path, graph_sha, weights_path, weights_sha):
    """Return the pinned captured input and row-major O weight bytes."""
    custody = custody_helper()
    require((capture_sha, graph_sha, weights_sha) == (CAPTURE_SHA, custody.GRAPH_SHA, custody.WEIGHTS_SHA),
            "exact retained R4 capture and baseline Compact graph/weights required")
    capture, graph, weights = map(custody.regular_path, (capture_path, model_path, weights_path))
    require(weights == graph.parent / custody.WEIGHTS_NAME, "O sidecar escaped its candidate graph directory")
    activation = capture_activation(json.loads(custody.read_pinned_bytes(capture, 32 << 20, capture_sha)), custody)
    model = onnx.load_model_from_string(custody.read_pinned_bytes(graph, 2 << 20, graph_sha))
    offset, size = output_weight_range(onnx, model.graph, custody)
    with weights.open("rb") as source:
        verified = custody.verified_weight_bytes(source, custody.WEIGHTS_SIZE, weights_sha, {"o": (2048, offset, size)})
    return activation, verified["o"][1]
