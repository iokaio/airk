# SPDX-License-Identifier: Apache-2.0
"""Load the fixed Compact R5 layer-zero GEMM operands without creating a session."""

import hashlib
import json
import math
import os
from pathlib import Path
import struct


CAPTURE_SHA = "8a3d7eef63fb0dca71fc5184bb37c151fa75903f95ecbf20dc39a29e09736d0e"
GRAPH_SHA = "971cf2d22d8bb7c874f3e5f5ce014bf2c494d1fb7245126d27d722223ac1577f"
WEIGHTS_SHA = "0c4bd89d032e7dd821071c3559776cd3da50d54d84b6e575373fd8cf7003a7c3"
RAW_SCORES_SHA = "2b064e2d847cb65723e3ac45863ae80fef87b029e43ddc423f554d001655227e"
INPUT_DIGEST = "ec7b4845c37fb7cbe2a64a866f8b6cdc03479d65593cdb0a1d01102fe56fc335"
CAPTURE_SCHEMA = "airk.private.compact-fp16-layer0-projection-taps/1"
WEIGHTS_NAME = "weights.bin"
WEIGHTS_SIZE = 3441397760
TOKENS = 32
HIDDEN = 2048
PROJECTIONS = {"k": 1024, "v": 1024, "q": 2048}
SHARED_RMS = "/scorer/layers.0/input_layernorm/Mul_1_output_0"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def routes(name):
    matmul = f"/scorer/layers.0/self_attn/{name}_proj/MatMul"
    prefix = "/airk_g5/weight/" + matmul
    return matmul, prefix + "/input16", prefix + "/output16"


def regular_path(path):
    path = Path(path)
    require(path.is_absolute() and path.resolve(strict=True) == path and path.is_file()
            and not any(part.is_symlink() or part.is_junction() for part in (path, *path.parents)),
            "expected a regular canonical absolute operand path")
    return path


def read_pinned_bytes(path, size_limit, expected):
    with path.open("rb") as source:
        raw = source.read(size_limit + 1)
    require(0 < len(raw) <= size_limit and hashlib.sha256(raw).hexdigest() == expected,
            "bounded operand source bytes or SHA-256 differ")
    return raw


def capture_operands(capture):
    require(isinstance(capture, dict) and capture.get("schema") == CAPTURE_SCHEMA
            and capture.get("complete") is capture.get("native_forward_returned") is
            capture.get("forward_synchronized") is True
            and capture.get("qualified") is capture.get("product_admission") is False
            and type(capture.get("numeric_forward_attempts")) is int
            and capture["numeric_forward_attempts"] == 1 and capture.get("error") is None,
            "exact synchronized native Compact projection capture required")
    mask_source = capture.get("source_mask")
    require(isinstance(mask_source, dict) and mask_source.get("verified_candidate_graph") == GRAPH_SHA
            and mask_source.get("compact_graph_uses_attention_mask_shape_only") is
            mask_source.get("mask_values_require_all_live_before_dispatch") is True,
            "native Compact capture graph or all-live policy differs")
    rows = capture.get("rows")
    require(isinstance(rows, list) and len(rows) == 1 and isinstance(rows[0], dict),
            "exact native projection score row required")
    row = rows[0]
    require(row.get("name") == "diagnostic-all-live-32"
            and type(row.get("tokens")) is int and row["tokens"] == TOKENS
            and row.get("input_digest") == INPUT_DIGEST, "frozen native T32 input differs")
    scores = row.get("raw_scores")
    require(isinstance(scores, list) and len(scores) == 8
            and all(type(value) in (float, int) and math.isfinite(value) for value in scores)
            and hashlib.sha256(struct.pack("<8f", *scores)).hexdigest() == RAW_SCORES_SHA,
            "sealed native score bits differ")

    roster = [(routes("k")[1], HIDDEN)] + [(routes(name)[2], width) for name, width in PROJECTIONS.items()]
    taps = capture.get("layer0_taps")
    require(isinstance(taps, list) and len(taps) == len(roster), "exact projection capture tap roster required")
    operands = []
    for tap, (name, width) in zip(taps, roster):
        require(isinstance(tap, dict) and set(tap) == {"name", "width", "bits"}
                and tap["name"] == name and type(tap["width"]) is int and tap["width"] == width,
                "projection tap name, order or width differs")
        bits = tap["bits"]
        require(isinstance(bits, list) and len(bits) == TOKENS * width
                and all(type(word) is int and 0 <= word <= 0xffff and (word & 0x7c00) != 0x7c00 for word in bits),
                "complete finite projection tap half words required")
        operands.append(struct.pack(f"<{TOKENS * width}H", *bits))
    return operands[0], dict(zip(PROJECTIONS, operands[1:]))


def external_range(tensor, sidecar_size):
    fields = {item.key: item.value for item in tensor.external_data}
    require(len(tensor.external_data) == len(fields) == 3
            and set(fields) == {"location", "offset", "length"} and fields["location"] == WEIGHTS_NAME,
            "external projection sidecar fields or location differ")
    offset_text, length_text = fields["offset"], fields["length"]
    require(all(value.isascii() and value.isdecimal() and len(value) <= 20
                and (value == "0" or not value.startswith("0")) for value in (offset_text, length_text)),
            "external sidecar offset and length must be canonical unsigned decimal")
    offset, length = int(offset_text), int(length_text)
    require(length > 0 and offset + length <= sidecar_size, "external sidecar range exceeds bounds")
    return offset, length


def projection_ranges(onnx, graph, sidecar_size):
    nodes = {node.name: node for node in graph.node}
    initializers = {tensor.name: tensor for tensor in graph.initializer}
    producers = {output: node.name for node in graph.node for output in node.output if output}
    require(len(nodes) == len(graph.node) and len(initializers) == len(graph.initializer)
            and len(producers) == sum(bool(output) for node in graph.node for output in node.output),
            "ambiguous projection graph node, initializer or value")

    fp16 = onnx.TensorProto.FLOAT16
    external = onnx.TensorProto.EXTERNAL
    spans = {tensor.name: external_range(tensor, sidecar_size)
             for tensor in graph.initializer if tensor.data_location == external}
    intervals = sorted(spans.values())
    require(all(offset + length <= next_offset
                for (offset, length), (next_offset, _) in zip(intervals, intervals[1:])),
            "external sidecar ranges overlap")
    selected = {}
    for name, width in PROJECTIONS.items():
        route, input_name, output_name = routes(name)
        node = nodes.get(route)
        require(node is not None and node.op_type == "MatMul" and node.domain == "" and not node.attribute
                and len(node.input) == 2 and node.input[0] == input_name and list(node.output) == [output_name],
                "selected FP16 projection MatMul route differs")
        cast = nodes.get(input_name)
        require(cast is not None and cast.op_type == "Cast" and cast.domain == ""
                and producers.get(input_name) == input_name and list(cast.input) == [SHARED_RMS]
                and list(cast.output) == [input_name] and len(cast.attribute) == 1
                and cast.attribute[0].name == "to" and cast.attribute[0].type == onnx.AttributeProto.INT
                and cast.attribute[0].i == fp16 and not cast.attribute[0].ref_attr_name,
                "selected FP16 projection inputs do not share the captured RMS result")
        weight = initializers.get(node.input[1])
        require(weight is not None and weight.name in spans and weight.data_type == fp16
                and tuple(weight.dims) == (HIDDEN, width)
                and {field.name for field, _ in weight.ListFields()} ==
                {"dims", "data_type", "name", "data_location", "external_data"},
                "selected projection B must be an external FP16 [2048,width] initializer")
        offset, length = spans[weight.name]
        require(offset % 2 == 0 and length == HIDDEN * width * 2, "selected projection B byte span differs")
        selected[name] = (width, offset, length)
    require(len({offset for _, offset, _ in selected.values()}) == len(selected),
            "selected projection B ranges are not distinct")
    return selected


def file_state(source):
    metadata = os.fstat(source.fileno())
    return metadata.st_dev, metadata.st_ino, metadata.st_size, metadata.st_mtime_ns, metadata.st_ctime_ns


def verified_weight_bytes(source, sidecar_size, expected_sha, selected):
    before = file_state(source)
    require(before[2] == sidecar_size and hashlib.file_digest(source, "sha256").hexdigest() == expected_sha,
            "projection sidecar length or SHA-256 differs")
    weights = {}
    for name, (width, offset, length) in selected.items():
        source.seek(offset)
        raw = source.read(length)
        require(len(raw) == length and all((high & 0x7c) != 0x7c for high in memoryview(raw)[1::2]),
                "selected projection B slice is short or nonfinite")
        weights[name] = (width, raw)
    require(file_state(source) == before, "projection sidecar changed during authentication or slicing")
    return weights


def load_operands(onnx, capture_path, capture_sha256, graph_path, graph_sha256, weights_path, weights_sha256):
    """Return captured input bytes, verified row-major B byte spans, and captured outputs."""
    require((capture_sha256, graph_sha256, weights_sha256) == (CAPTURE_SHA, GRAPH_SHA, WEIGHTS_SHA),
            "exact retained Compact R5 capture and corrected candidate pins required")
    capture = regular_path(capture_path)
    graph = regular_path(graph_path)
    weights = regular_path(weights_path)
    require(weights == graph.parent / WEIGHTS_NAME, "projection sidecar escaped the candidate graph directory")
    input_half_bytes, observed = capture_operands(json.loads(read_pinned_bytes(capture, 32 << 20, capture_sha256)))
    model = onnx.load_model_from_string(read_pinned_bytes(graph, 2 << 20, graph_sha256))
    selected = projection_ranges(onnx, model.graph, WEIGHTS_SIZE)
    with weights.open("rb") as source:
        weight_by_name = verified_weight_bytes(source, WEIGHTS_SIZE, weights_sha256, selected)
    return input_half_bytes, weight_by_name, observed
