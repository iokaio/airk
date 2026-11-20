# SPDX-License-Identifier: Apache-2.0
"""Literal half/FP32 rounding boundaries for the protected Compact MLP route."""
from pathlib import Path
import runpy
import unittest

try:
    import onnx
except ModuleNotFoundError as error:
    if error.name != "onnx":
        raise
    onnx = None

OWNER = runpy.run_path(str(Path(__file__).resolve().parents[1] / "arbiter-fp16-execution" / "stage_compact_down_projection.py"))


@unittest.skipIf(onnx is None, "ONNX is required for graph controls")
class DownGraphTests(unittest.TestCase):
    def baseline(self):
        nodes = []
        weights = []
        tensor = onnx.TensorProto
        for layer in range(28):
            base = f"/scorer/layers.{layer}/mlp"
            route = base + "/down_proj/MatMul"
            boundary = "/airk_g5/weight/" + route
            weight = tensor(name=f"down_weight_{layer}", data_type=tensor.FLOAT16, dims=[6144,2048], data_location=tensor.EXTERNAL)
            for key, value in (("location","weights.bin"),("length","25165824"),("offset",str(layer*25165824))):
                weight.external_data.add(key=key,value=value)
            weights.append(weight)
            nodes.extend([
                onnx.helper.make_node("Cast", [base+"/Mul_output_0"], [boundary+"/input16"],
                                      name=boundary+"/input16", to=tensor.FLOAT16),
                onnx.helper.make_node("MatMul", [boundary+"/input16",weight.name], [boundary+"/output16"], name=route),
                onnx.helper.make_node("Cast", [boundary+"/output16"], [route+"_output_0"],
                                      name=boundary+"/output32", to=tensor.FLOAT),
            ])
        return onnx.helper.make_model(onnx.helper.make_graph(nodes,"controls",[],[],weights))

    def test_inverse_recovers_every_unaffected_source_byte(self):
        baseline = self.baseline()
        candidate = OWNER["transform"](onnx,baseline)
        actual = OWNER["verify"](onnx,baseline,candidate)
        self.assertEqual(actual["new_input_and_weight_casts"],56)
        self.assertEqual(actual["removed_half_output_casts"],28)
        candidate.graph.initializer[4].external_data[1].value = "2"
        with self.assertRaisesRegex(ValueError,"inverse graph differs"):
            OWNER["verify"](onnx,baseline,candidate)

    def test_source_requantization_change_and_output16_substitution_refuse(self):
        baseline = self.baseline()
        candidate = OWNER["transform"](onnx,baseline)
        candidate.graph.node[3].output[0] = "unreviewed_output16"
        with self.assertRaisesRegex(ValueError,"invalid protected operands"):
            OWNER["verify"](onnx,baseline,candidate)
        baseline.graph.node[0].attribute[0].i = onnx.TensorProto.FLOAT
        with self.assertRaisesRegex(ValueError,"activation or rounding"):
            OWNER["transform"](onnx,baseline)


if __name__ == "__main__":
    unittest.main()
