# SPDX-License-Identifier: Apache-2.0
"""Regression controls for Large projection cut custody and exact row selection."""

import copy
import runpy
import unittest
from pathlib import Path


OWNER = runpy.run_path(str(Path(__file__).with_name("large_layer0_capture.py")))
SHARED = "/airk_g5/weight//scorer/layers.0/self_attn/k_proj/MatMul/input16"


def metadata():
    rows = []
    for i, (name, width) in enumerate(OWNER["TAPS"].items()):
        value = 100 + i
        node = i + 1
        rows.append({"kind": "value", "graph": 0, "value": value, "name": name, "rank": 3, "element_type": 10,
                     "provenance": "node_output", "producer": node})
        for axis, (number, symbol) in enumerate(((1,None),(None,"tokens"),(width,None))):
            rows.append({"kind":"dimension","graph":0,"index":axis,"value":value,"number":number,"symbol":symbol})
        node_name = SHARED if i == 0 else name.removeprefix("/airk_g5/weight/").removesuffix("/output16")
        rows.append({"kind":"node","graph":0,"node":node,"name":node_name,"ep":"CUDAExecutionProvider",
                     "op":"Cast" if i == 0 else "MatMul","domain":""})
        rows.append({"kind":"edge","graph":0,"node":node,"role":2,"slot":0,"value":value})
        if i:
            rows.append({"kind":"edge","graph":0,"node":node,"role":0,"slot":0,"value":100})
        rows.append({"kind":"edge","graph":0,"node":None,"role":4,"slot":i+1,"value":value})
    return {"complete":True,"failure":None,"rows":rows}


class Capture(unittest.TestCase):
    def test_requires_shared_live_gpu_operand_and_promoted_graph_outputs(self):
        baseline = metadata()
        self.assertEqual(len(OWNER["edges"](baseline, promoted=True)), 3)
        for field, value, select in (
            ("value", 105, lambda r: r["kind"]=="edge" and r["role"]==0),
            ("ep", "CPUExecutionProvider", lambda r: r["kind"]=="node" and r["op"]=="MatMul"),
            ("role", 0, lambda r: r["kind"]=="edge" and r["role"]==4),
            ("number", 2048, lambda r: r["kind"]=="dimension" and r["index"]==2),
        ):
            with self.subTest(field=field, value=value):
                bad = copy.deepcopy(baseline)
                next(r for r in bad["rows"] if select(r))[field] = value
                with self.assertRaises(ValueError):
                    OWNER["edges"](bad, promoted=True)

    def test_original_fp16_words_are_selected_in_declared_order(self):
        widths = (2560, 1024, 1024, 4096)
        self.assertEqual(tuple(OWNER["TAPS"].values()), widths)
        taps = [{"name": name, "width": width, "bits": [i for i in range(32) for _ in range(width)]}
                for name, width in OWNER["TAPS"].items()]
        selected = OWNER["selected_words"](taps)
        for name, words in selected.items():
            self.assertEqual(words, [[i] * OWNER["TAPS"][name] for i in (0,15,31)])
        taps[0]["bits"][0] = 0x7e00
        with self.assertRaises(ValueError):
            OWNER["selected_words"](taps)


if __name__ == "__main__":
    unittest.main()
