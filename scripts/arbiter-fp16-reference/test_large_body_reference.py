# SPDX-License-Identifier: Apache-2.0
"""Check all Large model body call sites on exact small operands before full CPU evaluation."""
import runpy
import unittest
from pathlib import Path

HERE=Path(__file__).resolve().parent
Base=runpy.run_path(str(HERE/"test_large_qkv_reference.py"))["TinyOperands"]
score=runpy.run_path(str(HERE/"large_body_reference.py"))["score"]

class Products(unittest.TestCase):
    def test_complete_layer_order_and_missing_tail(self):
        model=Base()
        sites=("self_attn.q","self_attn.k","self_attn.v","self_attn.o","mlp.gate","mlp.up","mlp.down")
        model.routes=[f"layers.{n}.{site}_proj.weight" for n in range(36) for site in sites]
        model.weights=dict.fromkeys(model.routes,next(iter(model.weights.values())))
        score(model,{"decision_position":31},lambda:None)
        self.assertEqual(model.calls,252)
        self.assertNotIn("_linear_half",model.__dict__)
        model.routes.pop()
        with self.assertRaisesRegex(ValueError,"did not complete"):
            score(model,{"decision_position":31},lambda:None)

if __name__=="__main__":
    unittest.main()
