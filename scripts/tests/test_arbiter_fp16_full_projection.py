# SPDX-License-Identifier: Apache-2.0
"""Controls for the Compact FP32 dense projections at the stock FP16 attention boundaries."""
from pathlib import Path
import runpy
import unittest

import onnx

OWNER = runpy.run_path(str(Path(__file__).resolve().parents[1] / "arbiter-fp16-execution" / "stage_compact_full_projection.py"))


def original():
    routes = [
        ("self_attn/q_proj", "input_layernorm/Mul_1_output_0", 2048, 2048),
        ("self_attn/k_proj", "input_layernorm/Mul_1_output_0", 2048, 1024),
        ("self_attn/v_proj", "input_layernorm/Mul_1_output_0", 2048, 1024),
        ("self_attn/o_proj", "self_attn/Reshape_7_output_0", 2048, 2048),
        ("mlp/up_proj", "post_attention_layernorm/Mul_1_output_0", 2048, 6144),
        ("mlp/gate_proj", "post_attention_layernorm/Mul_1_output_0", 2048, 6144),
        ("mlp/down_proj", "mlp/Mul_output_0", 6144, 2048),
    ]
    nodes, weights = [], []
    tensor = onnx.TensorProto
    make = onnx.helper.make_node
    for layer in range(28):
        prefix = f"/scorer/layers.{layer}/"
        for site, activation, rows, columns in routes:
            name = prefix + site + "/MatMul"
            stem = "/airk_g5/weight/" + name
            weight = tensor(name=name + "/weight", data_type=tensor.FLOAT16, dims=[rows, columns],
                            data_location=tensor.EXTERNAL)
            weight.external_data.add(key="location", value="weights.bin")
            weight.external_data.add(key="length", value=str(rows * columns * 2))
            weights.append(weight)
            nodes.extend([make("Cast", [prefix + activation], [stem + "/input16"], name=stem + "/input16", to=tensor.FLOAT16),
                          make("MatMul", [stem + "/input16", weight.name], [stem + "/output16"], name=name),
                          make("Cast", [stem + "/output16"], [name + "_output_0"], name=stem + "/output32", to=tensor.FLOAT)])
        nodes.append(make("GroupQueryAttention", ["q", "k", "v"], ["context"], name=prefix + "attention", domain="com.microsoft"))
    return onnx.helper.make_model(onnx.helper.make_graph(nodes, "full", [], [], weights))


class FullProjectionTests(unittest.TestCase):
    def test_original_dense_routes_and_attention_recovered(self):
        baseline = original()
        candidate = OWNER["transform"](onnx, baseline)
        proof = OWNER["verify"](onnx, baseline, candidate)
        self.assertEqual(proof["removed_activation_and_output_half_boundaries"], 392)
        self.assertEqual(sum(node.op_type == "GroupQueryAttention" for node in candidate.graph.node), 28)
        attention = next(node for node in candidate.graph.node if node.op_type == "GroupQueryAttention")
        attention.input[0] = "different_q"
        with self.assertRaisesRegex(ValueError, "unrelated source node changed"):
            OWNER["verify"](onnx, baseline, candidate)

    def test_immutable_weight_precision_and_actual_activation_required(self):
        baseline = original()
        candidate = OWNER["transform"](onnx, baseline)
        candidate.graph.node[1].input[0] = "rounded_half_activation"
        with self.assertRaisesRegex(ValueError, "new route or FP32 arithmetic differs"):
            OWNER["verify"](onnx, baseline, candidate)
        baseline.graph.initializer[0].data_type = onnx.TensorProto.FLOAT
        with self.assertRaisesRegex(ValueError, "projection storage ownership"):
            OWNER["transform"](onnx, baseline)


if __name__ == "__main__":
    unittest.main()
