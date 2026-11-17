# SPDX-License-Identifier: Apache-2.0
import unittest
from run_reviewed import validate_decision
class ReleaseTests(unittest.TestCase):
    def decision(self):return {'schema':'airk.private-selected-Context-source-preparation-r3-decision/1','approved':True,'protocol_sha256':'01'*32,'opaque_source_rubric_only':True,'neural_execution':False}
    def test_exact_scope_accepted(self):validate_decision(self.decision(),'01'*32)
    def test_template_not_approval(self):
        value=self.decision();value['approved']=None
        with self.assertRaises(ValueError):validate_decision(value,'01'*32)
    def test_foreign_protocol_refuses(self):
        with self.assertRaises(ValueError):validate_decision(self.decision(),'02'*32)
    def test_neural_scope_refuses(self):
        value=self.decision();value['neural_execution']=True
        with self.assertRaises(ValueError):validate_decision(value,'01'*32)
if __name__=='__main__':unittest.main()
