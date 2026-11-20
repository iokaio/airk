# SPDX-License-Identifier: Apache-2.0
"""Check grouped population, original recorder capacity and separate phase deadlines."""
import os
from pathlib import Path
import subprocess
import sys
import unittest
import uuid
from unittest.mock import patch

import run_grouped as grouped
from group_service import phase_deadline, settle_service
from installation_mutex import InstallationMutex, MUTEX_NAME
import test_dispatch_proof


class GroupContracts(unittest.TestCase):
    def setUp(self):
        ids = list(range(100))
        self.phases = [ids[i:i+8] for i in range(0,100,8)]
        self.groups = [[0,1,2,3],[4,5,6,7],[8,9,10,11,12]]

    @unittest.skipUnless(os.name == "nt","Windows installation mutex")
    def test_competing_process_refuses_until_actual_release(self):
        self.assertEqual(MUTEX_NAME, r"Local\AIRK-20261116-Focus1024-Reuse")
        name = "Local\\AIRK-CAL-test-" + uuid.uuid4().hex
        script = """import sys
from installation_mutex import InstallationMutex
try:
    with InstallationMutex(sys.argv[1]):
        print('acquired')
except BlockingIOError:
    print('busy')
"""
        def observe():
            completed = subprocess.run([sys.executable,"-B","-S","-c",script,name],cwd=Path(__file__).resolve().parent,
                                       capture_output=True,text=True,timeout=15,check=True)
            return completed.stdout.strip()
        with InstallationMutex(name):
            self.assertEqual(observe(),"busy")
        self.assertEqual(observe(),"acquired")

    def test_fixed_group_cardinality_and_order(self):
        self.assertEqual(grouped.group_roster(self.phases,self.groups), self.groups)
        self.groups[1][0] = 3
        with self.assertRaisesRegex(ValueError,"roster"):
            grouped.group_roster(self.phases,self.groups)

    def test_group_membership_cannot_change(self):
        self.groups[2].insert(0,self.groups[1].pop())
        with self.assertRaisesRegex(ValueError,"roster"):
            grouped.group_roster(self.phases,self.groups)

    def test_no_repeated_question(self):
        self.phases[8][0] = self.phases[0][0]
        with self.assertRaisesRegex(ValueError,"roster"):
            grouped.group_roster(self.phases,self.groups)

    def test_original_error_and_both_cleanup_errors_survive(self):
        receipt = []
        order = []
        def shutdown():
            order.append("shutdown")
            raise OSError("shutdown refused")
        def wait():
            order.append("wait")
            raise TimeoutError("wait expired")
        with self.assertRaisesRegex(ValueError,"phase refused") as observed:
            try:
                raise ValueError("phase refused")
            finally:
                settle_service(sys.exception(),shutdown,wait,receipt.append)
        self.assertEqual(order,["shutdown","wait"])
        self.assertEqual(receipt[0]["original"],{"type":"builtins.ValueError","message":"phase refused"})
        self.assertEqual([e["type"] for e in receipt[0]["settlement"]],["builtins.OSError","builtins.TimeoutError"])
        self.assertEqual(len(observed.exception.__notes__),2)

    def test_cleanup_failure_after_success_keeps_terminal_state_failed(self):
        receipt = []
        def wait():
            raise TimeoutError("native lease did not settle")
        with self.assertRaises(ExceptionGroup):
            settle_service(None,lambda:None,wait,receipt.append)
        self.assertIsNone(receipt[0]["original"])
        self.assertTrue(receipt[0]["terminal_failed"])
        self.assertEqual(receipt[0]["settlement"][0]["operation"],"wait")

    def test_deadlines_preserve_earlier_limit(self):
        self.assertEqual(phase_deadline(100,2000000),1800100)
        self.assertEqual(phase_deadline(100,125100),125100)

    def test_native_capacity_passes_exact_boundary_and_refuses_one_more(self):
        checker = test_dispatch_proof.JournalTests()
        checker.setUp()
        template = checker.rows
        checker.rows = [dict(row,dispatch_identity=i+1,sequence=i*5+row["sequence"])
                        for i in range(1024) for row in template]
        self.assertEqual(checker.verify()["focus_dispatches"],1024)
        checker.rows.append(dict(template[0],dispatch_identity=1025,sequence=5121))
        with self.assertRaisesRegex(ValueError,"dispatch limit"):
            checker.verify()

    def test_real_private_source_and_original_image_authenticate(self):
        plan = grouped.make_plan(123456,None)
        self.assertEqual(plan["phase_groups"],self.groups)
        self.assertEqual(plan["deadline_ms"]-plan["start_ms"]+plan["setup_elapsed_ms"],10800000)
        self.assertEqual(plan["recorder"]["admission_limit"],1024)
        self.assertEqual(plan["recorder"]["record_limit"],8192)

    def test_previous_phase_cannot_be_substituted_for_entire_lifetime(self):
        with self.assertRaisesRegex(ValueError,"incomplete native lifetime"):
            grouped.validate_chain([{},{}], self.groups, {}, {}, "publication")

    def test_terminal_gate_refuses_live_or_successful_prior_capture(self):
        prior = {"path":str(grouped.R4/"outer-owned.json"), "sha256":"0"*64}
        for code,settled in ((0,True),(1,False)):
            with self.subTest(code=code,settled=settled):
                terminal = {"exit_code":code, "owned_descendants_settled":settled,
                            "owned_job_final_active_processes":0,"owned_job_final_member_pids":[]}
                with patch.object(grouped,"linked", return_value=terminal):
                    with self.assertRaisesRegex(ValueError,"terminal|actual|incomplete"):
                        grouped.make_plan(123456,prior)


if __name__ == "__main__":
    unittest.main()
