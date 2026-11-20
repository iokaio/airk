# SPDX-License-Identifier: Apache-2.0
"""Verify original layer interval gates, interruptible custody and private creation privileges."""
import json
import os
from pathlib import Path
import runpy
from types import SimpleNamespace
import unittest

HERE = Path(__file__).resolve().parent
RUN = runpy.run_path(str(HERE/"run_large_t32.py"))
JOB = RUN["checked_module"](RUN["LAYER_JOB"],RUN["LAYER_JOB_SHA"])
OWNER = RUN["checked_module"](RUN["CONVERSION"],RUN["CONVERSION_SHA"])


class LayerJobTests(unittest.TestCase):
    def setUp(self):
        self.root = RUN["CAMPAIGN"]
        self.baseline = json.loads((self.root/"large-direct-fp32-independent-t1024-r1/reference.json").read_bytes())
        self.row = {"input":{"input_digest":self.baseline["input_digest"]}}
        self.args = SimpleNamespace(layers=[0,18],tokens=1024,train_phase=None,capture_hidden=False,prior_layers=None,
                                    prior_layers_sha256=None,layer_control=None,layer_control_sha256=None)

    def predecessor(self):
        return JOB["predecessor"](self.args,self.root,OWNER,self.row,self.baseline["candidate"],lambda: None)

    def test_original_uninterrupted_t1024_control(self):
        pins, rows, previous, expected = self.predecessor()
        self.assertEqual(len(pins),3)
        self.assertEqual(rows,[])
        self.assertIsNone(previous)
        self.assertEqual(expected,self.baseline["matching_fp16"])

    def test_no_predecessor_rejected_at_layer_18(self):
        self.args.layers = [18,36]
        with self.assertRaisesRegex(ValueError,"exact campaign Job receipt"):
            self.predecessor()

    def test_t8192_requires_observed_t1024_segmentation(self):
        self.args.tokens = 8192
        with self.assertRaisesRegex(ValueError,"exact campaign Job receipt"):
            self.predecessor()

    def test_single_interval_cannot_claim_segmentation(self):
        self.args.layers = [0,36]
        with self.assertRaisesRegex(ValueError,"layer interval"):
            self.predecessor()

    def test_only_one_exclusive_residual_destination_is_writable(self):
        handled = []
        dest = self.root/"not-created-layer-job-test.f32"
        hook = JOB["audit_hook"](lambda event,values,cache: handled.append((event,cache)),None,dest)
        hook("open",(str(dest),"x",os.O_CREAT|os.O_EXCL|os.O_WRONLY))
        self.assertEqual(handled,[])
        hook("open",(str(dest),"w",os.O_TRUNC|os.O_WRONLY))
        hook("open",(str(dest.with_suffix(".other")),"x",os.O_CREAT|os.O_EXCL|os.O_WRONLY))
        self.assertEqual(handled,[("open",None),("open",None)])


if __name__ == "__main__":
    unittest.main()
