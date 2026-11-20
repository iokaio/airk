# SPDX-License-Identifier: Apache-2.0
"""Exercise the Windows sampler on a live CPU child with no model loaded."""
import ctypes
from ctypes import wintypes
import subprocess
import sys
import time
from types import SimpleNamespace
import unittest

from wddm_capacity_snapshot import snapshot
from wddm_process_samples import ProcessSampler, observe_process


@unittest.skipUnless(sys.platform == "win32", "Windows WDDM campaign host only")
class ProcessSampleTests(unittest.TestCase):
    def test_owned_child_lifecycle(self):
        luid = snapshot(None)["dxgi"]["luid"]
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        wait = kernel.WaitForSingleObject
        wait.argtypes = [wintypes.HANDLE, wintypes.DWORD]
        wait.restype = wintypes.DWORD
        with subprocess.Popen(
                [sys.executable, "-I", "-B", "-S", "-c", "import sys; sys.stdin.buffer.read(1)"],
                stdin=subprocess.PIPE, creationflags=subprocess.CREATE_NO_WINDOW) as child:
            try:
                info = SimpleNamespace(process=int(child._handle), pid=child.pid)
                with self.assertRaisesRegex(ValueError, "handle and PID differ"):
                    ProcessSampler(SimpleNamespace(process=info.process, pid=child.pid + 1), luid)
                with ProcessSampler(info, luid) as sampler:
                    initial = sampler.sample()
                def signal_after_sample(handle, timeout):
                    if timeout and not child.stdin.closed:
                        child.stdin.close()
                    return wait(handle, timeout)
                result = observe_process(SimpleNamespace(WaitForSingleObject=signal_after_sample),
                                         info, time.monotonic() + 10, luid, initial)
                self.assertEqual(result["pid"], child.pid)
                self.assertEqual(result["before_resume"], initial)
                self.assertGreaterEqual(len(result["samples"]), 1)
                for row in [initial, *result["samples"]]:
                    self.assertGreater(row["private_commit_bytes"], 0)
                    self.assertGreater(row["adapter"]["Dedicated Usage"], 0)
                    self.assertGreaterEqual(row["end_monotonic_ns"], row["start_monotonic_ns"])
                self.assertEqual(child.wait(timeout=3), 0)
            finally:
                if not child.stdin.closed:
                    child.stdin.close()


if __name__ == "__main__":
    unittest.main()
