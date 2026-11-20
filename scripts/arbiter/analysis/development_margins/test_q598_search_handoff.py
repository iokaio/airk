# SPDX-License-Identifier: Apache-2.0
"""Check the Q598 callback against the original completed rehydration and failure chronology."""
from pathlib import Path
import runpy
from types import SimpleNamespace
import unittest

HERE = Path(__file__).resolve().parent
HANDOFF = runpy.run_path(str(HERE/"q598_search_handoff.py"))
REUSE = runpy.run_path(str(HERE/"reuse_installation.py"))


class Handoff(unittest.TestCase):
    def test_observed_original_recovery(self):
        output = REUSE["E"]/"train-q598-closed-control-recovery-r2"
        owner = REUSE["read"](output/"owned.json","e07b84a680a7d4c53b92ae91e02de2feed727566b78e87de0abaa14558b752c8")
        ready=REUSE["linked"](owner["shared_installation"]["ready"])
        request=REUSE["read"](output/"restart-request.json")
        retained=[]
        native={"wait_observed":lambda folder,callback,*a,**kw:callback(), "write_json":lambda path,value:retained.append(value)}
        api=HANDOFF["before_setup"](native,output,request,REUSE["CONTROL"])
        args=(output,lambda:True,SimpleNamespace(pid=ready["health"]["process_id"]),1,"pilot observer readiness")
        self.assertTrue(api["wait_observed"](*args))
        self.assertEqual([r["outcome"] for r in retained[0]["completions"]],["ready","ready"])
        with self.assertRaisesRegex(ValueError,"second"):
            api["wait_observed"](*args)

    def test_failed_status_still_shuts_down(self):
        output = REUSE["E"]/"compact-development-TRAIN_Q598-focus1024-run-r1"
        calls = []
        def run(argv,logs,label,*args):
            calls.append(label)
            if label=="failure-stage-at-shutdown": raise RuntimeError("status unavailable")
            return "closed"
        control = REUSE["CONTROL"]
        native={"wait_observed":None,"run_logged":run}
        api=HANDOFF["before_setup"](native,output,{},control)
        with self.assertRaisesRegex(RuntimeError,"status unavailable"):
            api["run_logged"](["cli","--output","json","--service-runtime-dir",str(control),"service","shutdown"],
                              output/"commands","shutdown",{},1)
        self.assertEqual(calls,["failure-stage-at-shutdown","shutdown"])

    def test_waits_for_windows_writer_release(self):
        import ctypes
        from ctypes import wintypes
        path=REUSE["E"]/"compact-development-TRAIN_Q598-focus1024-run-r2/fixture/ready.json"
        api=ctypes.WinDLL("kernel32",use_last_error=True)
        fn=api.CreateFileW
        fn.argtypes=[wintypes.LPCWSTR,wintypes.DWORD,wintypes.DWORD,wintypes.LPVOID,wintypes.DWORD,wintypes.DWORD,wintypes.HANDLE]
        fn.restype=wintypes.HANDLE
        handle=fn(str(path),0x80000000,0,None,3,0x80,None)
        self.assertNotEqual(handle,ctypes.c_void_p(-1).value)
        try:
            self.assertIsNone(HANDOFF["published"](path))
        finally:
            api.CloseHandle.argtypes=[wintypes.HANDLE]
            self.assertTrue(api.CloseHandle(handle))
        self.assertEqual(HANDOFF["published"](path),REUSE["read"](path))

    def test_stale_predecessor_refuses(self):
        with self.assertRaisesRegex(ValueError,"foreign Search predecessor"):
            HANDOFF["predecessor"](REUSE,REUSE["R3"]/"closed.json","0"*64,{})


if __name__ == "__main__":
    unittest.main()
