# SPDX-License-Identifier: Apache-2.0
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import cutlass_study_inputs as study


class ExactStudy(unittest.TestCase):
    def test_requires_every_original_ordinal_in_order(self):
        rows = [{"id":str(i),"ordinal":i} for i in range(100)]
        study.require_full(rows)
        for altered in (rows[:-1],rows[1:],list(reversed(rows)),rows[:-1]+[rows[0]],
                        rows[:-1]+[dict(rows[-1],id="0")]):
            with self.subTest(count=len(altered)), self.assertRaises(ValueError):
                study.require_full(altered)

    def test_references_cannot_be_missing_or_a_different_role(self):
        with self.assertRaisesRegex(ValueError,"complete ordered"):
            study.references([], "protected_matching", [], {})
        job = {"exit_code":0,"timed_out":False,"log_oversized":False,
               "child_assigned_before_resume":True,"owned_descendants_settled":True,
               "owned_job_forced_termination":False,"owned_job_final_active_processes":0,
               "owned_job_final_member_pids":[]}
        batch = {"schema":"airk.private.compact-protected-cal-cpu-batch/1","first":0,"count":1,
                 "complete":True,"role":"publisher_fp32","owned":job}
        with patch.object(study,"json_pin",return_value=batch), self.assertRaisesRegex(ValueError,"wrong reference"):
            study.references([{}],"protected_matching",[],{})

    def test_partial_group_does_not_reach_certificate_or_fit(self):
        with patch.object(study,"json_pin",return_value={"terminal":[],"lifetimes":["first"]}), self.assertRaisesRegex(ValueError,"natural closure"):
            study.prepare({"full":{}})


    def test_wrong_native_source_cannot_issue_a_study(self):
        from run_cutlass_study import image_from
        receipt = {"passed":True,"inputs_unchanged":True,"command_inputs_unchanged":True,
                   "command":["cargo","test","--no-run"],"log":"cargo.log",
                   "groups":{"native":{"files":{p:"foreign" for p in __import__("run_cutlass_study").OWNERS}}}}
        descriptor = {"build":{"path":"/build/build.json"},"cargo_log":{"path":"/build/cargo.log"}}
        with patch("run_cutlass_study.json_pin",return_value=receipt), patch("run_cutlass_study.checked",return_value=b""), self.assertRaisesRegex(ValueError,"source changed"):
            image_from(descriptor)


if __name__ == "__main__":
    unittest.main()
