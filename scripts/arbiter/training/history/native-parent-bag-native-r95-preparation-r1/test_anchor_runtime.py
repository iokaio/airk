# SPDX-License-Identifier: Apache-2.0
"""Cheap refusal controls only; no native process, ACL mutation or model load."""
from pathlib import Path
import unittest
import tempfile
import anchor_runtime as owner

class AnchorRefusals(unittest.TestCase):
    def test_relative_foreign_parent_and_unselected_stage_refuse(self):
        for prefix, stage in [("relative", "emit"), (str(owner.ROOT / "wrong"), "emit"),
                              (str(owner.MODELS / "native-parent-bag-native-private-anchor-r1"), "old-final205")]:
            with self.assertRaises(ValueError): owner.anchor_path({"private_anchor_prefix":prefix},stage)
    def test_only_six_reviewed_stages_have_owned_paths(self):
        packet={"private_anchor_prefix":str(owner.MODELS / "native-parent-bag-native-private-anchor-r1")}
        self.assertEqual(owner.anchor_path(packet,"numeric").parent,owner.MODELS)
        self.assertEqual(owner.anchor_path(packet,"numeric").name,
                         "native-parent-bag-native-private-anchor-r1-numeric")
    def test_missing_forged_foreign_and_changed_proof_refuse(self):
        path=owner.MODELS / "native-parent-bag-native-private-anchor-r1-emit"
        valid={"schema":"airk.private-qasper-final-anchor-observation/1","canonical_path":str(path),
               "current_sid":"SID-actual-test-only","owner_sid":"SID-actual-test-only","protected":True,
               "no_reparse":True,"current_owner_only":True,"rules":[{"inert":True}],"sddl":"inert-test-only"}
        self.assertEqual(owner.validate_proof(valid,path),valid)
        for key,value in [("protected",False),("current_owner_only",False),("no_reparse",False),
                          ("owner_sid","foreign"),("canonical_path",str(path.parent)),("rules",[])]:
            changed=dict(valid);changed[key]=value
            with self.assertRaises(ValueError):owner.validate_proof(changed,path)
        with self.assertRaises(ValueError):owner.validate_proof({},path)
        with self.assertRaises(ValueError):owner.require_same(valid,{**valid,"sddl":"changed"})
    def test_existing_anchor_cannot_be_adopted(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(ValueError): owner.require_fresh(Path(directory))
            owner.require_fresh(Path(directory)/"fresh")

if __name__=="__main__":unittest.main()
