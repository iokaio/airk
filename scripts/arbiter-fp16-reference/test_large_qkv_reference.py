# SPDX-License-Identifier: Apache-2.0
"""Guard the Large matching recipe's exact call order and binary16 boundaries."""

import runpy
import unittest
from pathlib import Path

import torch


SCORE = runpy.run_path(str(Path(__file__).with_name("large_qkv_reference.py")))["score"]


class TinyOperands:
    layers, hidden, qheads, kvheads, dim = 36, 2560, 32, 8, 128
    device = "cpu"

    def __init__(self, flip=False):
        self.torch = torch
        self.flip = flip
        self.routes = [f"layers.{i}.self_attn.{q}_proj.weight" for i in range(36) for q in "qkv"]
        w = torch.tensor([[0.5,0.25],[1.,-1.]],dtype=torch.float16)
        self.weights = dict.fromkeys(self.routes,w)
        self.calls = 0

    def _linear_half(self, _value, _name):
        raise AssertionError("protected route escaped")

    def forward(self, row, *, context_flash_single_tile):
        assert context_flash_single_tile and row["decision_position"]==31
        x = torch.tensor([[2.,4.]],dtype=torch.float16)
        for name in reversed(self.routes) if self.flip else self.routes:
            y = self._linear_half(x,name)
            assert y.dtype==torch.float16 and y.tolist()==[[2.,-2.]]
            self.calls+=1
        return y


class Protected(unittest.TestCase):
    def test_visits_108_real_route_names_once_in_model_order(self):
        model=TinyOperands()
        SCORE(model,{"decision_position":31},lambda: None)
        self.assertEqual(model.calls,108)
        self.assertNotIn("_linear_half",model.__dict__)

    def test_reversed_route_order_refused(self):
        model=TinyOperands(flip=True)
        with self.assertRaisesRegex(ValueError,"in all 36"):
            SCORE(model,{"decision_position":31},lambda: None)
        self.assertNotIn("_linear_half",model.__dict__)


if __name__=="__main__":
    unittest.main()
