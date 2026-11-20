# SPDX-License-Identifier: Apache-2.0
"""Regression controls for the Windows Large operator and full-model debug records."""
from pathlib import Path
import runpy
import unittest

observe = runpy.run_path(str(Path(__file__).with_name("probe_large_cutlass.py")))["observation"]
MARKER = 'AIRK_LARGE_COMPENSATED={"job":{},"native":{}}'
SCRATCH = ("flash=0, lean=0, efficient=1, fused_runner=0, fused_cross=0, bias=0, "
           "attn_bias=0, mask_dims=0, has_qkv_workspace=0, workspace=0, past=0, present=0")

def sample(names):
    records = []
    for name in names:
        records += [f"Operator=MultiHeadAttention Node={name} DataType=fp32 SdpaKernel=EFFICIENT_ATTENTION", SCRATCH]
    return "\n".join([*records, MARKER, "test result: ok. 1 passed; 0 failed"]).encode()

class LargeDispatch(unittest.TestCase):
    def test_operand_precision_and_workspace(self):
        data = sample(["airk_large_cutlass_t32_operand"])
        self.assertEqual(len(observe(data, "operand")[1]), 1)
        for source in [data.replace(b"fp32", b"fp16"), data.replace(b"workspace=0, past", b"workspace=1, past")]:
            with self.assertRaises(ValueError):
                observe(source, "operand")

    def test_graph_cannot_dispatch_and_score_requires_all_distinct_layers(self):
        names = [f"/airk_large_cutlass/{i}/efficient" for i in range(36)]
        self.assertEqual(len(observe(sample(names), "score")[1]), 36)
        self.assertEqual(observe(sample([]), "graph")[1], [])
        for data, mode in [(sample(names), "graph"), (sample(names[:-1] + names[:1]), "score"),
                           (sample(names[:-1]), "score"), (sample([]), "score")]:
            with self.assertRaises(ValueError):
                observe(data, mode)

    def test_unprofiled_populations_reject_operator_traces(self):
        for mode in ("fixed", "mixed"):
            self.assertEqual(observe(sample([]), mode)[1:], ([], []))
            with self.assertRaises(ValueError):
                observe(sample(["/airk_large_cutlass/0/efficient"]), mode)

if __name__ == "__main__":
    unittest.main()
