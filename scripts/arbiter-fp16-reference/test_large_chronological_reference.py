# SPDX-License-Identifier: Apache-2.0
"""CPU operand controls for irregular lengths, native mask/roster limits and 32:8 grouping."""
from pathlib import Path
import runpy
import unittest

import torch

HERE = Path(__file__).resolve().parent
adapter = runpy.run_path(str(HERE / "large_chronological_reference.py"))
validate_native = runpy.run_path(str(HERE.parent / "arbiter-fp16-reference.py"))["validate_native"]


def native_row(tokens, options):
    return {"input_ids": [1] * tokens, "attention_mask": [1] * tokens, "position_ids": list(range(tokens)),
            "decision_position": tokens - 1, "candidate_positions": list(range(len(options))), "option_mask": options}


class Operands:
    """Tiny learned-weight substitutes exercise the 108 routes; no publisher model is opened."""
    focus = False
    layers, hidden, qheads, kvheads, dim = 36, 2560, 32, 8, 128
    config = {"vocab_size": 100}
    torch = torch

    def __init__(self):
        self.weights = {f"layers.{i}.self_attn.{projection}_proj.weight": torch.ones(1, 1, dtype=torch.float16)
                        for i in range(36) for projection in ("q", "k", "v")}

    def _linear_half(self, x, name):
        return x

    def forward(self, row, *, context_flash_single_tile=False):
        q = torch.zeros(1, 32, len(row["input_ids"]), 128, dtype=torch.float16)
        k = q[:, :8]
        for i in range(36):
            for projection in ("q", "k", "v"):
                self._linear_half(k[0, 0, :1, :1], f"layers.{i}.self_attn.{projection}_proj.weight")
            causal_half_attention(torch, q, k, k, row["attention_mask"], context_flash_single_tile=context_flash_single_tile)
        return torch.tensor([[i if live else -10000. for i, live in enumerate(row["option_mask"])]], dtype=torch.float32)


def causal_half_attention(*args, **kwargs):
    raise AssertionError("wide operation did not invoke the selected arithmetic")


class ChronologicalReference(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1)

    def test_irregular_causal_tail_and_head_groups(self):
        for n in (33, 63, 64, 65, 79, 129):
            with self.subTest(tokens=n):
                query = torch.zeros(1, 32, n, 128, dtype=torch.float16)
                keys = torch.zeros(1, 8, n, 128, dtype=torch.float16)
                head = 16 * torch.arange(8).reshape(1, 8, 1, 1)
                token = torch.arange(n).reshape(1, 1, n, 1)
                values = (head + token).half().expand_as(keys)
                actual = adapter["causal_attention"](torch, query, keys, values, [1]*n, lambda: None)
                expected = (16*(torch.arange(32)//4).reshape(1, 32, 1, 1) + token/2).half().expand_as(actual)
                self.assertTrue(torch.equal(actual, expected))

    def test_partial_tile_keeps_scaled_maximum_and_future_values_out(self):
        import math
        n = 67
        query = torch.zeros(1, 32, n, 128, dtype=torch.float16)
        query[..., 0] = 1
        keys = torch.zeros(1, 8, n, 128, dtype=torch.float16)
        keys[..., 64, 0] = 8
        values = torch.zeros_like(keys)
        values[..., 64, :] = 4
        actual = adapter["causal_attention"](torch, query, keys, values, [1]*n, lambda: None)
        self.assertTrue((actual[..., :64, :] == 0).all())
        # One distinct logit after 64 zeros. Softmax weights are rounded to half
        # only for the value product; the online denominator keeps full precision.
        for token in (64, 65, 66):
            expected = torch.tensor(4 / (1 + token * math.exp(-8 / math.sqrt(128))), dtype=torch.float16)
            self.assertTrue((actual[..., token, :] == expected).all())

    def test_varied_original_card_count_and_disabled_options(self):
        for widen, options in ((False, [True]), (True, [True, False, True, True, False, True, True]),
                               (True, [False]*7 + [True])):
            model = Operands()
            actual = adapter["score"](model, native_row(33, options), lambda: None, widen_qkv=widen)
            self.assertEqual(actual.tolist(), [[i if live else -10000. for i, live in enumerate(options)]])
            self.assertNotIn("forward", model.__dict__)
            self.assertNotIn("_linear_half", model.__dict__)

    def test_non_native_masks_and_rosters_refuse_before_projection(self):
        for key, value in (("attention_mask", [0] + [1]*32), ("attention_mask", [1]*12 + [0] + [1]*20),
                           ("position_ids", [0]*33), ("option_mask", [False]), ("option_mask", [True]*9)):
            with self.subTest(field=key), self.assertRaises(ValueError):
                adapter["score"](Operands(), native_row(33, [True]) | {key: value}, lambda: None, widen_qkv=True)


if __name__ == "__main__":
    unittest.main()
