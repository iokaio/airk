# SPDX-License-Identifier: Apache-2.0
"""Independent literal controls for Compact's four-output scientific ABI."""
from pathlib import Path
import runpy
from types import SimpleNamespace
import unittest
from unittest.mock import patch

VERIFY = runpy.run_path(str(Path(__file__).resolve().parents[1] / "arbiter-fp16-execution" / "probe_compact.py"))["validate_full_cut"]
HEAD = [
    ("/scorer/layers.0/input_layernorm/Mul_1_output_0", 2048),
    ("/scorer/layers.13/input_layernorm/Mul_1_output_0", 2048),
    ("/scorer/layers.27/input_layernorm/Mul_1_output_0", 2048),
    ("/scorer/norm/Mul_1_output_0", 2048),
]
ATTENTION = [(f"/airk_g5/attention/13/{leaf}", width) for leaf, width in
             (("q16", 2048), ("k16", 1024), ("v16", 1024), ("raw16", 2048))]
DENSE = [("/airk_g5/attention/13/raw16", 2048),
         ("/scorer/layers.13/self_attn/o_proj/MatMul_output_0", 2048),
         ("/scorer/layers.13/mlp/Mul_output_0", 6144),
         ("/scorer/layers.13/mlp/down_proj/MatMul_output_0", 2048)]


class FourOutputTests(unittest.TestCase):
    def setUp(self):
        self.prior = {"rows": [{"raw_scores": [1.0, 2.0], "compiled": {"input_digest": "original"}}]}
        self.base = SimpleNamespace(read=lambda _path: self.prior)
        self.patch = patch.dict(VERIFY.__globals__, exact_pin=lambda *_args: {"path": "sealed"})
        self.patch.start()
        self.addCleanup(self.patch.stop)

    def record(self, group, roster):
        rows = []
        for name, width in roster:
            tokens = 1014 if name in ("/airk_g5/attention/13/v16", "/airk_g5/attention/13/k16") else 1
            words = [0] * (tokens * width * (2 if name.startswith("/scorer/") else 1))
            rows.append({"name": name, "width": width, "tokens_retained": tokens, "words": words})
        return {"rows": [{"raw_scores": [1.0, 2.0], "compiled": {"input_digest": "original"}}],
                "full_cut_group": group, "full_cut_rows": rows}

    def test_three_complete_declared_rosters_and_exact_words(self):
        for group, roster in (("head", HEAD), ("attention", ATTENTION), ("dense", DENSE)):
            value = self.record(group, roster)
            VERIFY(value, self.base, group)
            value["rows"][0]["raw_scores"][0] = 1.00001
            with self.assertRaisesRegex(ValueError, "perturbed"):
                VERIFY(value, self.base, group)

    def test_refuses_wrong_role_and_truncation(self):
        value = self.record("head", HEAD)
        with self.assertRaisesRegex(ValueError, "group identity"):
            VERIFY(value, self.base, "dense")
        value["full_cut_rows"][0]["words"].pop()
        with self.assertRaisesRegex(ValueError, "payload"):
            VERIFY(value, self.base, "head")
        value["full_cut_rows"].pop()
        with self.assertRaisesRegex(ValueError, "roster"):
            VERIFY(value, self.base, "head")


if __name__ == "__main__":
    unittest.main()
