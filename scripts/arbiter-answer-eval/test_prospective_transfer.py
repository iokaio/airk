# SPDX-License-Identifier: Apache-2.0
import json
from types import SimpleNamespace
import unittest

from finalist_joint_evidence import Evidence
import finalist_gather_contract as contract
import prospective_transfer as actor
import prospective_transfer_grade as grading


class TransferGradeTests(unittest.TestCase):
    def test_source_verified_union_and_absent_need_stay_separate(self):
        path = "part.rs"
        text = "line one\nline two\n"
        h = actor.digest(text.encode())
        repo = SimpleNamespace(texts={path:text.splitlines()}, digests={path:bytes.fromhex(h)})
        args = {"task":"explain","needs":["one","two"]}
        passage = Evidence("part.rs:1-2",text)
        outcome = actor.render_outcome(repo,args,[passage],1,"c"*64,contract.render,[])
        refs = [{"need":1,"required":[[{"path":path,"lines":[1,2],"source_sha256":h,"excerpt_sha256":h}]]},
                {"need":2,"required":[],"absent":True}]
        result = grading.grade_arm(repo,outcome,refs,args["needs"])
        self.assertEqual(result["counts"]["whole_needs"],1)
        self.assertEqual(result["counts"]["attributed_whole_needs"],0)
        self.assertEqual(result["counts"]["absence_not_individually_scored"],1)
        self.assertEqual(result["counts"]["absence_explicit_no_candidate"],0)
        rpc = json.loads(outcome["wire"])
        body = json.loads(rpc["result"]["content"][0]["text"])
        body["passages"][0]["text"] = "different text"
        rpc["result"]["content"][0]["text"] = json.dumps(body)
        corrupt = json.dumps(rpc)
        bad = dict(outcome,wire=corrupt,wire_bytes=len(corrupt.encode()),wire_sha256=actor.digest(corrupt.encode()))
        with self.assertRaisesRegex(ValueError,"bytes do not support"):
            grading.grade_arm(repo,bad,refs,args["needs"])
