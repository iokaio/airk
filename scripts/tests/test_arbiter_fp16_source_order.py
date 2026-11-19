# SPDX-License-Identifier: Apache-2.0
"""Literal CPU controls for Compact's optional source-order arithmetic."""

import importlib.util
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import patch

try:
    import torch
except ModuleNotFoundError as error:
    if error.name != "torch":
        raise
    torch = None


@unittest.skipIf(torch is None, "Torch is required for source-order arithmetic controls")
class SourceOrderControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        root = Path(__file__).resolve().parents[2]
        path = root / "scripts/arbiter-fp16-execution/verify_projection_candidate_t32.py"
        spec = importlib.util.spec_from_file_location("compact_source_order_controls", path)
        cls.owner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.owner)

    def test_fma_corrects_midpoint_that_binary64_and_split_round_down(self):
        value = torch.tensor([1 + 2 ** -12], dtype=torch.float32)
        accumulator = torch.tensor([2 ** -100], dtype=torch.float32)
        lower = torch.tensor([1 + 2 ** -11], dtype=torch.float32)
        upper = torch.tensor([1 + 2 ** -11 + 2 ** -23], dtype=torch.float32)
        self.assertTrue(torch.equal(value.square() + accumulator, lower))
        self.assertTrue(torch.equal((value.double().square() + accumulator.double()).float(), lower))
        self.assertTrue(torch.equal(self.owner.fp32_square_fma(torch, accumulator, value), upper))

    def test_lane_groups_then_warp_merge_expected_binary32_sum(self):
        for width, expected in ((128, 16_777_340), (2048, 16_779_260)):
            with self.subTest(width=width):
                row = torch.ones((1, width), dtype=torch.float32)
                row[0, 0] = 4096
                result = self.owner.source_order_square_mean(torch, row)
                self.assertEqual(tuple(result.shape), (1, 1))
                self.assertEqual(result.item() * width, expected)

    def test_quickgelu_subtractive_negative_branch(self):
        values = torch.tensor([-20, -1, 0, 1, 20], dtype=torch.float32)
        result = self.owner.source_order_sigmoid(torch, values)
        self.assertEqual(result[[0, 2, 4]].tolist(), [0, .5, 1])
        self.assertGreater(torch.sigmoid(values)[0].item(), 0)
        self.assertEqual((result[1] + result[3]).item(), 1.0)

    def test_output_projection_owns_a_separate_recipe_and_exact_operand_route_census(self):
        projected = tuple(f"layers.{layer}.self_attn.{kind}_proj.weight"
                          for layer in range(28) for kind in ("q", "k", "v", "o"))
        self.assertEqual(self.owner.OUTPUT_ROUTES, projected)
        self.assertEqual(self.owner.source_order_recipe(False, True),
                         {"qkvo_fp32_routes": list(projected), "context_flash_single_tile": True})
        self.assertEqual(self.owner.reference_identity(False, True)[3:],
                         ("compact-qkvo-fp32-t32-reference.json", "matching_qkvo_fp32"))
        self.assertEqual(self.owner.reference_identity(False, False)[3:],
                         ("compact-qkv-fp32-t32-reference.json", "matching_qkv_fp32"))
        with self.assertRaisesRegex(ValueError, "only one"):
            self.owner.reference_identity(True, True)

        class Literal:
            layers, hidden, qheads, kvheads, dim, device = 28, 2048, 16, 8, 128, "cpu"

            def __init__(self):
                self.torch = torch
                self.weights = dict.fromkeys(projected, torch.tensor([[1, 2], [3, 4]], dtype=torch.float16))
                self.route_calls = projected
                self.delegated = []

            def _linear_half(self, value, name):
                self.delegated.append(name)
                return value

            def forward(self, row, *, context_flash_single_tile):
                assert row == "literal" and context_flash_single_tile is True
                value = torch.tensor([[2, 3]], dtype=torch.float16)
                for name in self.route_calls:
                    result = self._linear_half(value, name)
                    assert result.dtype == torch.float16 and result.tolist() == [[8, 18]]
                assert self._linear_half(value, "layers.0.mlp.gate_proj.weight") is value
                return torch.full((1, 8), 1.25, dtype=torch.float32)

        literal = Literal()
        scores = self.owner.projection_scores(literal, "literal", lambda: None, routes=projected)
        self.assertEqual(scores, [1.25] * 8)
        self.assertEqual(literal.delegated, ["layers.0.mlp.gate_proj.weight"])
        self.assertNotIn("_linear_half", literal.__dict__)
        literal.route_calls = projected[:-1]
        with self.assertRaisesRegex(ValueError, "112"):
            self.owner.projection_scores(literal, "literal", lambda: None, routes=projected)

    def test_exact_census_and_scope_restoration(self):
        shapes = {(1, 32, 2048): 57, (1, 32, 16, 128): 28, (1, 32, 8, 128): 28}
        helper = SimpleNamespace(rms=lambda *_: None)
        model = SimpleNamespace(torch=torch)
        prior = helper.rms
        gate = torch.zeros((1, 32, 6144), dtype=torch.float32)
        rms_calls = {shape: (torch.ones(shape, dtype=torch.float32),
                             torch.ones((shape[-1],), dtype=torch.float32)) for shape in shapes}

        def forward(independent, row, check):
            self.assertIs(independent, model)
            self.assertEqual(row, "literal")
            for shape, count in shapes.items():
                for _ in range(count):
                    helper.rms(model.torch, *rms_calls[shape], 1e-6)
            for _ in range(28):
                model.torch.sigmoid(gate)
            return [0.0] * 8

        # The reduction and sigmoid have separate arithmetic controls.
        # Keep this control limited to the worker's interception and trace.
        def mean(_torch, value):
            return torch.ones_like(value[..., :1])

        with patch.object(self.owner, "projection_scores", side_effect=forward), \
                patch.object(self.owner, "source_order_square_mean", side_effect=mean), \
                patch.object(self.owner, "source_order_sigmoid", return_value=gate):
            _, counts = self.owner.source_order_scores(helper, model, "literal", lambda: None)
        self.assertEqual(counts, {"rms": [{"shape": list(shape), "calls": count} for shape, count in shapes.items()],
                                  "quickgelu_calls": 28})
        self.assertIs(model.torch, torch)
        self.assertIs(helper.rms, prior)
        with patch.object(self.owner, "projection_scores", return_value=[0.0] * 8), \
                self.assertRaisesRegex(ValueError, "113 RMSNorm and 28 QuickGelu"):
            self.owner.source_order_scores(helper, model, "literal", lambda: None)
        self.assertIs(model.torch, torch)
        self.assertIs(helper.rms, prior)


if __name__ == "__main__":
    unittest.main()
