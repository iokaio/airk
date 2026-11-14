# SPDX-License-Identifier: Apache-2.0
import ctypes
import importlib.util
from pathlib import Path
import unittest

SPEC=importlib.util.spec_from_file_location("darwin_wait",Path(__file__).parents[1]/"darwin-waitid.py")
MODULE=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(MODULE)


class DarwinWaitIdTests(unittest.TestCase):
    def test_layout_matches_independently_checked_sdk_constants(self):
        self.assertEqual(ctypes.sizeof(MODULE.Siginfo),104)
        self.assertEqual(MODULE.Siginfo.si_code.offset,8)
        self.assertEqual(MODULE.Siginfo.si_pid.offset,12)
        self.assertEqual(MODULE.Siginfo.si_status.offset,20)

    def test_noevent_is_not_completion(self):
        self.assertIsNone(MODULE.decode(MODULE.Siginfo(),42))

    def test_exact_pid_and_real_waitid_exit_reason_are_required(self):
        info=MODULE.Siginfo();info.si_pid=42;info.si_code=1;info.si_status=7
        self.assertEqual(vars(MODULE.decode(info,42)),{"si_pid":42,"si_code":1,"si_status":7})
        for pid,code,status in [(43,1,7),(42,5,7),(42,1,-1),(42,1,256)]:
            info.si_pid=pid;info.si_code=code;info.si_status=status
            with self.subTest(pid=pid,code=code,status=status),self.assertRaises(ValueError):MODULE.decode(info,42)


if __name__=="__main__":unittest.main()
