# SPDX-License-Identifier: Apache-2.0
"""Check independent baseline identity and the Large graph-only placement boundary."""
from pathlib import Path
import runpy
import unittest

owner = runpy.run_path(str(Path(__file__).with_name("probe_large.py")))


class LargeProbeTests(unittest.TestCase):
    @staticmethod
    def graph():
        nodes = [{"name": f"/airk_g5/attention/{index}/efficient", "op": "GroupQueryAttention",
                  "domain": "com.microsoft", "ep": "CUDAExecutionProvider", "subgraph_index": 0} for index in range(36)]
        return {"complete": True, "nodes": nodes, "reported_nodes": 36}

    def test_complete_large_attention_required(self):
        graph = self.graph()
        owner["validate_large_attention"](graph)
        for key, value in (("name", "/airk_g5/attention/0/efficient"), ("op", "Attention"),
                           ("ep", "CPUExecutionProvider"), ("domain", "ai.onnx")):
            with self.subTest(key=key), self.assertRaises(ValueError):
                changed = self.graph()
                changed["nodes"][-1][key] = value
                owner["validate_large_attention"](changed)

    def test_compact_roster_is_rejected(self):
        graph = self.graph()
        graph["nodes"] = graph["nodes"][:28]
        graph["reported_nodes"] = 28
        with self.assertRaises(ValueError):
            owner["validate_large_attention"](graph)

    def test_t32_requires_actual_all_layer_flash_records_and_recomputed_scores(self):
        ref = {"input_digest": "fixed", "matching_fp16": [1.0] + [0.0] * 7, "original_fp32": [1.0] + [0.0] * 7}
        row = {"tokens": 32, "input_digest": "fixed", "raw_scores": ref["matching_fp16"],
               "development_screen_pass": True, "matching_reference_0_001": True}
        for name in ("matching_fp16", "original_fp32"):
            row.update({name + "_raw_error": 0.0, name + "_mass_error": 0.0, name + "_argmax_matches": True})
        value = {"schema": "airk.private.large-fp16-scientific-t32/1", "complete": True, "qualified": False,
                 "product_admission": False, "session_teardown_observed": True, "native_host_charge_release_observed": True,
                 "native_control_cleanup_observed": True, "process_cuda_poisoned": False, "rows": [row],
                 "profiled_runs": [{"tokens": 32, "node_count": 2234, "cpu_nodes": 478, "cpu_max_tensor_bytes": 32}]}
        logs = [f"Operator=GroupQueryAttention Node=/airk_g5/attention/{i}/efficient DataType=fp16 SdpaKernel=FLASH_ATTENTION"
                for i in range(36)]
        shared = runpy.run_path(str(Path(__file__).with_name("probe_compact.py")))
        result = owner["scoring_result"](value, ref, "\n".join(logs), shared)
        self.assertTrue(result["development_screen_pass"])
        self.assertTrue(result["release_matching_raw_pass"])
        self.assertEqual(result["release_matching_raw_limit"], .007)
        with self.assertRaises(ValueError):
            owner["scoring_result"](value, ref, "\n".join(logs[1:]), shared)
        for drift, expected_pass in ((.006, True), (.008, False)):
            row["raw_scores"] = [1.0 + drift] + [0.0] * 7
            for name in ("matching_fp16", "original_fp32"):
                metrics = shared["development_metrics"](row["raw_scores"], ref[name])
                row[name + "_raw_error"] = metrics["raw_error"]
                row[name + "_mass_error"] = metrics["mass_error"]
            row["matching_reference_0_001"] = False
            verdict = owner["scoring_result"](value, ref, "\n".join(logs), shared)
            self.assertEqual(verdict["release_matching_raw_pass"], expected_pass)
            self.assertTrue(verdict["development_screen_pass"])
        row["matching_fp16_raw_error"] = 0.3
        with self.assertRaises(ValueError):
            owner["scoring_result"](value, ref, "\n".join(logs), shared)

    def test_fp32_receipt_is_rejected(self):
        with self.assertRaises(ValueError):
            owner["metadata_contract"]({"schema": "airk.private.large-fp32-batch-view-metadata/1"}, {})


if __name__ == "__main__":
    unittest.main()
