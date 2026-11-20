# SPDX-License-Identifier: Apache-2.0
"""CPU-only native observation, deadline and device identity regression checks."""
import os
import select
import subprocess
import sys
import time
import unittest

from fp16_operations import verify as verify_operations
from fp16_supervisor import guarded_observation, native_settled


class StaticAdapter:
    def sample(self):
        return {"used": 0, "free": 1024, "total": 1024}


class Supervisor(unittest.TestCase):
    def test_kernel_pidfd_deadline_kills_only_owned_process(self):
        with subprocess.Popen([sys.executable, "-IB", "-c", "import signal;signal.pause()"]) as child:
            receipt = {}
            class WaitingAdapter(StaticAdapter):
                def sample(self):
                    # Block the sampler until the observed process actually exits.
                    fd = os.pidfd_open(child.pid)
                    try:
                        poll = select.poll()
                        poll.register(fd, select.POLLIN)
                        if not poll.poll(2500):
                            raise RuntimeError("independent watchdog failed to terminate scorer")
                    finally:
                        os.close(fd)
                    return super().sample()
            with self.assertRaises(TimeoutError):
                guarded_observation(child, WaitingAdapter(), time.monotonic() + 0.3,
                                    StaticAdapter().sample(), [], receipt)
            self.assertIs(receipt.get("forced_termination"), True)
            self.assertLess(child.wait(timeout=2), 0)

    def test_observed_single_pid_and_complete_exit(self):
        # Managed held pipe makes the child exit only when independent kernel sampling begins.
        rd, wr = os.pipe()
        samples = []
        class Observed(StaticAdapter):
            def sample(self):
                nonlocal wr
                if wr >= 0:
                    os.close(wr)
                    wr = -1
                return super().sample()
        try:
            with subprocess.Popen([sys.executable, "-IB", "-c", "import os,sys;os.read(int(sys.argv[1]),1)",
                                   str(rd)], pass_fds=(rd,)) as child:
                gpu = Observed()
                rc = guarded_observation(child, gpu, time.monotonic() + 5,
                                         StaticAdapter().sample(), samples, {})
                self.assertEqual(rc, 0)
                self.assertTrue(samples and samples[0]["host"]["VmRSS"] > 0)
        finally:
            os.close(rd)
            if wr >= 0:
                os.close(wr)

    def test_journal_rejects_foreign_clock_or_changed_deadline(self):
        value = {"rows": [], "request": {"sha256":"input"}, "profile":"profile", "pid":72,
                 "native_binary_sha256":"image", "original_expires_monotonic_ns":2000,
                 "load_native_call_us":1}
        common = {"schema":"airk.arbiter.native-operation/1", "kind":"load", "ordinal":0,
                  "identity":{"input_sha256":"input", "profile":"profile", "process_id":72,
                              "native_binary_sha256":"image"},
                  "clock":"unix_monotonic", "operation_input_digest":None,
                  "expires_monotonic_ns":2000, "started_monotonic_ns":1}
        start = common | {"phase":"started", "sequence":1, "observed_monotonic_ns":1,
                          "native_success":None, "native_call_elapsed_us":None}
        end = common | {"phase":"returned", "sequence":2, "observed_monotonic_ns":1500,
                        "native_success":True, "native_call_elapsed_us":1}
        self.assertEqual(verify_operations("graph", value, [start, end], []), [])
        for changed in ({"clock":"windows_performance_counter"}, {"expires_monotonic_ns":2001},
                        {"native_success":False}, {"observed_monotonic_ns":2000}):
            with self.subTest(changed=changed), self.assertRaises(ValueError):
                verify_operations("graph", value, [start, end | changed], [])

    def test_native_settlement_requires_measured_adapter_and_real_release(self):
        key = "GPU-00000000-0000-0000-0000-000000000001"
        adapter = {"uuid": [0] * 15 + [1]}
        value = dict(pid=742, native_binary_sha256="a", complete=True,
                     session_teardown_observed=True, device_teardown_observed=True,
                     native_control_cleanup_observed=True, native_host_charge_release_observed=True,
                     native_control_retained_for_unknown_cuda=False, process_cuda_poisoned=False,
                     adapter_before=adapter, adapter_after=adapter)
        native_settled(value, 742, "a", key)
        for mutation in ({"device_teardown_observed": False}, {"native_control_cleanup_observed": False},
                         {"adapter_after": {"uuid": [0] * 16}}, {"pid": 743}, {"process_cuda_poisoned": True}):
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                native_settled(value | mutation, 742, "a", key)


if __name__ == "__main__":
    unittest.main()
