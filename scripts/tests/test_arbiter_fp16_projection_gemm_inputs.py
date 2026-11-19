# SPDX-License-Identifier: Apache-2.0
"""In-memory custody and geometry controls for the Compact projection operand loader."""

import hashlib
import importlib.util
import io
from pathlib import Path
import struct
import unittest
from unittest.mock import patch

try:
    import onnx
except ModuleNotFoundError as error:
    if error.name != "onnx":
        raise
    raise unittest.SkipTest("ONNX is required for Compact projection operand controls") from error

from onnx import TensorProto, helper


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "projection_gemm_inputs", ROOT / "scripts/arbiter-fp16-execution/projection_gemm_inputs.py",
)
M = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)
DIGEST = "ec7b4845c37fb7cbe2a64a866f8b6cdc03479d65593cdb0a1d01102fe56fc335"
RMS = "/scorer/layers.0/input_layernorm/Mul_1_output_0"


def projection_fixture():
    nodes, weights = [], []
    offset = 0
    for name, width in [("k", 1024), ("v", 1024), ("q", 2048)]:
        route = f"/scorer/layers.0/self_attn/{name}_proj/MatMul"
        cast = "/airk_g5/weight/" + route + "/input16"
        output = "/airk_g5/weight/" + route + "/output16"
        weight = TensorProto(name=name, dims=[2048, width], data_type=TensorProto.FLOAT16,
                             data_location=TensorProto.EXTERNAL)
        length = 4096 * width
        for key, value in [("location", "weights.bin"), ("offset", str(offset)), ("length", str(length))]:
            weight.external_data.add(key=key, value=value)
        nodes += [helper.make_node("Cast", [RMS], [cast], name=cast, to=TensorProto.FLOAT16),
                  helper.make_node("MatMul", [cast, name], [output], name=route)]
        weights.append(weight)
        offset += length
    return helper.make_graph(nodes, "literal-projections", [], [], weights), offset


def capture_fixture():
    roster = [
        ("/airk_g5/weight//scorer/layers.0/self_attn/k_proj/MatMul/input16", 2048, 0x8000),
        ("/airk_g5/weight//scorer/layers.0/self_attn/k_proj/MatMul/output16", 1024, 0x0001),
        ("/airk_g5/weight//scorer/layers.0/self_attn/v_proj/MatMul/output16", 1024, 0xbc00),
        ("/airk_g5/weight//scorer/layers.0/self_attn/q_proj/MatMul/output16", 2048, 0x3c00),
    ]
    return {
        "schema": "airk.private.compact-fp16-layer0-projection-taps/1",
        "complete": True, "native_forward_returned": True, "forward_synchronized": True,
        "qualified": False, "product_admission": False, "numeric_forward_attempts": 1,
        "source_mask": {
            "verified_candidate_graph": M.GRAPH_SHA,
            "compact_graph_uses_attention_mask_shape_only": True,
            "mask_values_require_all_live_before_dispatch": True,
        },
        "rows": [{
            "name": "diagnostic-all-live-32", "tokens": 32, "input_digest": DIGEST,
            "raw_scores": [-0.0, 1, 0, -2, 3, 4, -5, 6],
        }],
        "layer0_taps": [{"name": name, "width": width, "bits": [bits] * (32 * width)}
                        for name, width, bits in roster],
    }


class ProjectionInputControls(unittest.TestCase):
    def test_immutable_approved_pins_refuse_before_files(self):
        with patch.object(M, "regular_path", side_effect=AssertionError("must not touch paths")):
            with self.assertRaisesRegex(ValueError, "exact retained Compact R5"):
                M.load_operands(onnx, "capture", "a" * 64, "graph", M.GRAPH_SHA, "weights", M.WEIGHTS_SHA)

    def test_native_rows_round_trip_little_endian_without_changing_half_bits(self):
        capture = capture_fixture()
        seal = hashlib.sha256(bytes.fromhex(
            "000000800000803f00000000000000c000004040000080400000a0c00000c040",
        )).hexdigest()
        with patch.object(M, "RAW_SCORES_SHA", seal):
            shared, observed = M.capture_operands(capture)
            self.assertEqual(shared, b"\x00\x80" * 65536)
            self.assertEqual(observed, {
                "k": b"\x01\x00" * 32768, "v": b"\x00\xbc" * 32768, "q": b"\x00\x3c" * 65536,
            })
            capture["layer0_taps"][1]["bits"][0] = 0x7e00
            with self.assertRaisesRegex(ValueError, "finite projection tap"):
                M.capture_operands(capture)
            capture["rows"][0]["raw_scores"][0] = 0.0
            with self.assertRaisesRegex(ValueError, "sealed native score bits"):
                M.capture_operands(capture)
        capture["rows"][0]["input_digest"] = "a" * 64
        with self.assertRaisesRegex(ValueError, "frozen native T32"):
            M.capture_operands(capture)

    def test_graph_resolves_exact_b_and_shared_cast_order(self):
        graph, size = projection_fixture()
        self.assertEqual(M.projection_ranges(onnx, graph, size), {
            "k": (1024, 0, 4194304), "v": (1024, 4194304, 4194304), "q": (2048, 8388608, 8388608),
        })
        graph.node[2].input[0] = RMS + "/other"
        with self.assertRaisesRegex(ValueError, "do not share the captured RMS"):
            M.projection_ranges(onnx, graph, size)

    def test_external_alias_escaped_range_non_half_and_mixed_storage_refuse(self):
        for mutate, reason in [
            (lambda graph: setattr(graph.initializer[0].external_data[0], "value", "../weights.bin"), "location"),
            (lambda graph: setattr(graph.initializer[1].external_data[1], "value", "0"), "ranges overlap"),
            (lambda graph: setattr(graph.initializer[2].external_data[1], "value", "8388610"), "exceeds bounds"),
            (lambda graph: setattr(graph.initializer[0].external_data[1], "value", "00"), "unsigned decimal"),
            (lambda graph: setattr(graph.initializer[2].external_data[2], "value", "8388606"), "B byte span"),
            (lambda graph: setattr(graph.initializer[2], "data_type", TensorProto.FLOAT), "external FP16"),
            (lambda graph: setattr(graph.initializer[2], "raw_data", b"x"), "external FP16"),
        ]:
            graph, size = projection_fixture()
            mutate(graph)
            with self.subTest(reason=reason), self.assertRaisesRegex(ValueError, reason):
                M.projection_ranges(onnx, graph, size)

    def test_hash_then_slice_from_same_handle_and_refuse_nonfinite(self):
        raw = struct.pack("<8H", 0x7c00, 0, 0x3c00, 0xbc00, 0, 0x0001, 0x8000, 0x7c00)
        expected_hash = hashlib.sha256(raw).hexdigest()
        file_state = (0, 0, len(raw), 0, 0)
        with patch.object(M, "file_state", return_value=file_state):
            weights = M.verified_weight_bytes(io.BytesIO(raw), len(raw), expected_hash, {
                "k": (1, 4, 4), "v": (1, 10, 4),
            })
            self.assertEqual(weights, {"k": (1, b"\x00\x3c\x00\xbc"), "v": (1, b"\x01\x00\x00\x80")})
            with self.assertRaisesRegex(ValueError, "sidecar length or SHA"):
                M.verified_weight_bytes(io.BytesIO(raw), len(raw), "0" * 64, {"k": (1, 4, 4)})
            with self.assertRaisesRegex(ValueError, "short or nonfinite"):
                M.verified_weight_bytes(io.BytesIO(raw), len(raw), expected_hash, {"k": (1, 0, 4)})
