# SPDX-License-Identifier: Apache-2.0
"""Independent tests for the private protected-attention capture and comparison."""
from pathlib import Path
import runpy
import unittest

OWNER = runpy.run_path(str(Path(__file__).resolve().parents[1] / "arbiter-fp16-execution/probe_compact_cutlass.py"))
KERNEL = "Operator=MultiHeadAttention Node=airk_cutlass_gqa_decision_control DataType=fp32 SdpaKernel=EFFICIENT_ATTENTION"
WORKSPACE = "flash=0, lean=0, efficient=1, fused_runner=0, fused_cross=0, bias=0, attn_bias=0, mask_dims=0, has_qkv_workspace=0, workspace=0, past=0, present=0"


class CutlassTests(unittest.TestCase):
    def test_two_measured_dispatches_and_separate_zero_workspaces(self):
        lines = [KERNEL, WORKSPACE, KERNEL, WORKSPACE, 'AIRK_COMPACT_CUTLASS_OPERAND={"count":2}', "test result: ok. 1 passed; 0 failed"]
        feed = lambda parts: "\n".join(parts).encode()
        value, kernels, scratch = OWNER["native_output"](feed(lines))
        self.assertEqual((value, kernels, scratch), ({"count":2}, [KERNEL]*2, [WORKSPACE]*2))
        for position, replacement in ((2, KERNEL.replace("fp32", "fp16")), (3, WORKSPACE.replace("workspace=0, past", "workspace=99, past"))):
            changed = lines.copy()
            changed[position] = replacement
            with self.assertRaises(ValueError):
                OWNER["native_output"](feed(changed))

    def test_release_ceiling_and_reversed_near_tie(self):
        compare = OWNER["numerical"]
        self.assertTrue(compare([.006, 1.], [0., 1.], [0., 1.])["release_numerical_case_pass"])
        larger = compare([.008, 1.], [0., 1.], [0., 1.])
        self.assertFalse(larger["release_numerical_case_pass"])
        self.assertTrue(larger["development_numerical_case_pass"])
        reversed_tie = compare([.001, 0.], [0., .001], [0., .001])
        self.assertFalse(reversed_tie["development_numerical_case_pass"])
        self.assertFalse(reversed_tie["matching"]["winner_agrees"])
        with self.assertRaises(ValueError):
            compare([float("nan")], [0.], [0.])


class TimingTests(unittest.TestCase):
    def test_fixed_population_uses_twenty_warm_samples_and_complete_words(self):
        timing=runpy.run_path(str(Path(__file__).resolve().parents[1]/"arbiter-fp16-execution/compact_cutlass_timing.py"))
        scores=[0.5]*8
        sealed=[timing["words"](scores)]*4
        rows=[{"tokens":8192,"cycle":0,"kind":"warmup" if i==0 else "sample","raw_scores":scores.copy(),
               "native_call_us":i+1,"start_us":i*100,"finish_us":i*100+25,"start_utc_ns":100*i,
               "finish_utc_ns":100*i+25,"forward_synchronized":True} for i in range(21)]
        summary=timing["summarize"]("fixed",rows,sealed)
        self.assertEqual((summary["samples"],summary["p95_us"]),(20,20))
        rows[12]["raw_scores"][7]=0.49999
        with self.assertRaisesRegex(ValueError,"score words"):
            timing["summarize"]("fixed",rows,sealed)
        with self.assertRaises(ValueError):
            timing["summarize"]("mixed",rows,sealed)


if __name__ == "__main__":
    unittest.main()
