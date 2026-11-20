# SPDX-License-Identifier: Apache-2.0
import runpy
import unittest
from pathlib import Path

M = runpy.run_path(str(Path(__file__).with_name("prepare_large.py")))


class LargePreparationTests(unittest.TestCase):
    def test_large_storage_and_tile(self):
        config = dict(num_hidden_layers=36, num_attention_heads=32, num_key_value_heads=8,
                      head_dim=128, hidden_size=2560, intermediate_size=9728, vocab_size=151936)
        measured = M["reference_memory"](config)
        self.assertEqual(measured["matching_reference_weight_bytes"], 8045328384)
        self.assertEqual(measured["largest_8192_attention_score_tile_bytes"], 67108864)
        config["num_attention_heads"] = 16
        with self.assertRaisesRegex(ValueError, "Large geometry"):
            M["reference_memory"](config)

    def fixture(self):
        rows = []
        for width, digest in M["DIGESTS"].items():
            source = dict(input_ids=list(range(width)), input_digest=digest, attention_mask=[1] * width,
                          position_ids=list(range(width)), decision_position=width-1,
                          candidate_positions=list(range(8)), option_mask=[True] * 8)
            rows.append(dict(name=str(width), input=source, reference_original_fp32=[0.0] * 8))
        native = {"rows": [dict(tokens=n, name=str(n), input_digest=d) for n, d in M["DIGESTS"].items()]}
        pin = {"path": "sample", "sha256": "sample"}
        shared = dict(model="arbiter-context-qwen3-4b", rows=rows, compiler="compiler", original_reference_assets=[])
        fixture = dict(shared, schema="airk.arbiter.native-numeric/2", reference_producer=pin, resource_policy="limits")
        producer = dict(shared, schema="airk.arbiter.independent-native-numeric-reference/2", resource_policy_digest="limits")
        return [(fixture, producer, pin)], native

    def test_rejects_compact_128_input_even_under_large_model_name(self):
        pairs, native = self.fixture()
        self.assertEqual(sorted(M["selected_rows"](pairs, native)), [32, 128, 1024, 8192])
        pairs[0][0]["rows"][1]["input"]["input_digest"] = "791067affc1eeba53660a0044369f22f1f463535656419c5da9a486517222c7a"
        with self.assertRaisesRegex(ValueError, "Large native input"):
            M["selected_rows"](pairs, native)

    def test_checks_native_order_and_eight_complete_scores(self):
        pairs, native = self.fixture()
        native["rows"].reverse()
        with self.assertRaisesRegex(ValueError, "Large CUDA FP32 input"):
            M["selected_rows"](pairs, native)
        pairs[0][0]["rows"][0]["reference_original_fp32"].pop()
        with self.assertRaisesRegex(ValueError, "original scores"):
            M["selected_rows"](pairs, native)


if __name__ == "__main__":
    unittest.main()
