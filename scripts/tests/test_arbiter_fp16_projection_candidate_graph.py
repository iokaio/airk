# SPDX-License-Identifier: Apache-2.0
"""In-memory controls for Compact's independent projection graph verifier."""

from pathlib import Path
import runpy
import unittest

try:
    import onnx
except ModuleNotFoundError as error:
    if error.name != "onnx":
        raise
    onnx = None


ROOT = Path(__file__).resolve().parents[2]
VERIFY = ROOT / "scripts/arbiter-fp16-execution/verify_projection_candidate_graph.py"


@unittest.skipIf(onnx is None, "ONNX is required for projection graph controls")
class ProjectionCandidateGraphControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.verify = staticmethod(runpy.run_path(str(VERIFY))["verify_projection_candidate_graph"])

    def fixture(self, *, output_projection=False):
        helper, tensor = onnx.helper, onnx.TensorProto
        inputs = [helper.make_tensor_value_info(name, tensor.INT64, [1, 32])
                  for name in ("input_ids", "attention_mask", "position_ids",
                               "decision_positions", "candidate_positions")]
        inputs.append(helper.make_tensor_value_info("option_mask", tensor.BOOL, [1, 8]))
        outputs = [helper.make_tensor_value_info("scores", tensor.FLOAT, [1, 8])]
        baseline, proposed, weights = [], [], []
        offset = 0
        previous = "input_ids"
        for layer in range(28):
            root = f"/scorer/layers.{layer}"
            norm = root + "/input_layernorm/Mul_1_output_0"
            stem = root + "/self_attn/"
            normalize = helper.make_node("Cast", [previous], [norm], name=root + "/norm", to=tensor.FLOAT)
            baseline.append(normalize)
            proposed.append(normalize)
            projections = (("q", 2048), ("k", 1024), ("v", 1024))
            if output_projection:
                projections += (("o", 2048),)
            for kind, width in projections:
                activation = norm
                if kind == "o":
                    gqa = f"/airk_g5/attention/{layer}"
                    raw = gqa + "/raw16"
                    activation = stem + "Reshape_7_output_0"
                    attention = helper.make_node("GroupQueryAttention", [previous] * 3, [raw],
                                                  name=gqa + "/efficient", domain="com.microsoft")
                    restore = helper.make_node("Cast", [raw], [activation], name=gqa + "/output32", to=tensor.FLOAT)
                    baseline.extend((attention, restore))
                    proposed.extend((attention, restore))
                name = stem + f"{kind}_proj/MatMul"
                old = "/airk_g5/weight/" + name
                new = "/airk_compact_qkv_fp32" + name
                a16, out16 = old + "/input16", old + "/output16"
                a32, b32, out32 = new + "/input32", new + "/weight32", new + "/output32"
                b16 = f"projection_{layer}_{kind}"
                weight = tensor(name=b16, dims=[2048, width], data_type=tensor.FLOAT16,
                                data_location=tensor.EXTERNAL)
                length = 4096 * width
                for key, value in (("location", "weights.bin"), ("offset", str(offset)), ("length", str(length))):
                    weight.external_data.add(key=key, value=value)
                weights.append(weight)
                offset += length

                narrow_input = helper.make_node("Cast", [activation], [a16], name=a16, to=tensor.FLOAT16)
                baseline.extend((narrow_input, helper.make_node("MatMul", [a16, b16], [out16], name=name)))
                proposed.append(narrow_input)
                proposed.extend((
                    helper.make_node("Cast", [a16], [a32], name=a32, to=tensor.FLOAT),
                    helper.make_node("Cast", [b16], [b32], name=b32, to=tensor.FLOAT),
                    helper.make_node("MatMul", [a32, b32], [out32], name=name),
                    helper.make_node("Cast", [out32], [out16], name=new + "/output16", to=tensor.FLOAT16),
                ))
                previous = out16
                if kind == "o":
                    previous = name + "_output_0"
                    result_cast = helper.make_node("Cast", [out16], [previous], name=old + "/output32", to=tensor.FLOAT)
                    baseline.append(result_cast)
                    proposed.append(result_cast)
        final_node = helper.make_node("Cast", [previous], ["scores"], name="readout", to=tensor.FLOAT)
        baseline.append(final_node)
        proposed.append(final_node)
        return [
            helper.make_model(helper.make_graph(nodes, "literal Compact projection scope", inputs, outputs, weights),
                              opset_imports=[helper.make_opsetid("", 18)], producer_name="literal")
            for nodes in (baseline, proposed)
        ]

    def test_exact_model_delta_is_accepted_without_using_emitter(self):
        source, proposal = self.fixture()
        self.assertEqual(self.verify(source, proposal, onnx),
                         {"projection_count": 84, "added_cast_count": 252,
                          "baseline_node_count": 197, "candidate_node_count": 449})

    def test_opt_in_o_delta_preserves_original_attention_and_result_casts(self):
        source, proposal = self.fixture(output_projection=True)
        self.assertEqual(self.verify(source, proposal, onnx, output_projection=True),
                         {"projection_count": 112, "added_cast_count": 336,
                          "baseline_node_count": 337, "candidate_node_count": 673})
        with self.assertRaisesRegex(ValueError, "exactly three added Casts"):
            self.verify(source, proposal, onnx)

    def test_opt_in_rejects_a_changed_original_attention_restoration(self):
        source, proposal = self.fixture(output_projection=True)
        for graph in (source.graph, proposal.graph):
            restore = next(node for node in graph.node if node.name == "/airk_g5/attention/0/output32")
            restore.attribute[0].i = onnx.TensorProto.FLOAT16
        with self.assertRaisesRegex(ValueError, "exact Cast /airk_g5/attention/0/output32"):
            self.verify(source, proposal, onnx, output_projection=True)

    def test_alias_type_and_edge_changes_refuse(self):
        def mutate(model, suffix, change):
            node = next(item for item in model.graph.node
                        if item.name.startswith("/airk_compact_qkv_fp32") and item.name.endswith(suffix))
            change(node)

        for suffix, change in (
            ("/input32", lambda n: setattr(n.attribute[0], "i", onnx.TensorProto.FLOAT16)),
            ("/weight32", lambda n: n.input.__setitem__(0, "projection_0_k")),
            ("/output16", lambda n: n.output.__setitem__(0, "foreign_value")),
            ("/output16", lambda n: n.MergeFromString(b"\x98\x06\x01")),
        ):
            with self.subTest(suffix=suffix), self.assertRaisesRegex(ValueError, "exact Cast"):
                source, proposal = self.fixture()
                mutate(proposal, suffix, change)
                self.verify(source, proposal, onnx)
        source, proposal = self.fixture()
        matmul = next(n for n in proposal.graph.node if n.op_type == "MatMul")
        matmul.input[:] = list(reversed(matmul.input))
        with self.assertRaisesRegex(ValueError, "exact float projection"):
            self.verify(source, proposal, onnx)

    def test_model_storage_abi_and_protected_nodes_refuse(self):
        for change in (
            lambda m: setattr(m, "producer_name", "different"),
            lambda m: setattr(m.opset_import[0], "version", 19),
            lambda m: setattr(m.graph.initializer[0].external_data[1], "value", "2"),
            lambda m: setattr(m.graph.output[0], "name", "other_scores"),
            lambda m: m.graph.value_info.add().CopyFrom(onnx.helper.make_tensor_value_info(
                "invented", onnx.TensorProto.FLOAT, [1])),
        ):
            source, proposal = self.fixture()
            change(proposal)
            with self.assertRaisesRegex(ValueError, "metadata, graph declarations, or initializer storage"):
                self.verify(source, proposal, onnx)
        source, proposal = self.fixture()
        proposal.graph.node[-1].input[0] = "projection_0_q"
        with self.assertRaisesRegex(ValueError, "protected node changed"):
            self.verify(source, proposal, onnx)

    def test_scope_rejects_extra_or_missing_projection_nodes(self):
        source, proposal = self.fixture()
        proposal.graph.node.pop()
        with self.assertRaisesRegex(ValueError, "exactly three added Casts"):
            self.verify(source, proposal, onnx)
        source, proposal = self.fixture()
        source.graph.node[2].name = "/scorer/layers.28/self_attn/q_proj/MatMul"
        with self.assertRaisesRegex(ValueError, "projection census"):
            self.verify(source, proposal, onnx)


if __name__ == "__main__":
    unittest.main()
