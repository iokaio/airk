# SPDX-License-Identifier: Apache-2.0
"""Regression for Darwin zombie-only killpg EPERM; no compiler/native build."""
import contextlib
import importlib.util
from pathlib import Path
import tempfile
import types
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("unix_owner", Path(__file__).parents[1] / "provision-static-onnx-runtime.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class UnixOwnerTests(unittest.TestCase):
    def test_exact_zombie_batch_is_one_bounded_read_and_rejects_crossed_or_missing_rows(self):
        valid = b"42 40 501 Z\n43 40 501 ZN\n"
        with patch.object(MODULE.os, "getuid", return_value=501, create=True), patch.object(MODULE.time, "monotonic", return_value=10):
            for output, accepted in ((valid, True), (b"42 41 501 Z\n43 40 501 Z\n", False),
                                     (b"42 40 502 Z\n43 40 501 Z\n", False), (b"42 40 501 S\n43 40 501 Z\n", False),
                                     (b"42 40 501 Z\n", False), (b"42 40 501 Z\n42 40 501 Z\n", False)):
                result = types.SimpleNamespace(returncode=0, stdout=output, stderr=b"")
                with patch.object(MODULE.subprocess, "run", return_value=result) as invoke:
                    if accepted:
                        self.assertEqual(len(MODULE.darwin_zombie_members([42, 43], 40, 11)), 2)
                        self.assertEqual(invoke.call_args.args[0][2], "42,43")
                        self.assertEqual(invoke.call_args.kwargs["timeout"], 1)
                    else:
                        with self.assertRaises(ValueError):
                            MODULE.darwin_zombie_members([42, 43], 40, 11)
            with patch.object(MODULE.subprocess, "run") as invoke:
                with self.assertRaises(ValueError):
                    MODULE.darwin_zombie_members([42], 40, 9)
                invoke.assert_not_called()

    def run_fixture(self, census):
        signals = []
        process = types.SimpleNamespace(pid=4242, wait=lambda timeout: 0)
        def signal(group, value):
            self.assertEqual(group, process.pid)
            signals.append(value)
            if value == 0:
                raise ProcessLookupError()
        with contextlib.ExitStack() as stack:
            for name, value in {"waitid": lambda *args: types.SimpleNamespace(si_pid=4242, si_code=1, si_status=0),
                                "P_PID": 1, "WEXITED": 4, "WNOHANG": 1, "WNOWAIT": 16, "killpg": signal}.items():
                stack.enter_context(patch.object(MODULE.os, name, value, create=True))
            stack.enter_context(patch.object(MODULE.sys, "platform", "darwin"))
            stack.enter_context(patch.object(MODULE.signal, "SIGKILL", 9, create=True))
            stack.enter_context(patch.object(MODULE.subprocess, "Popen", return_value=process))
            stack.enter_context(patch.object(MODULE, "darwin_group_members", side_effect=census))
            stack.enter_context(patch.object(MODULE.time, "sleep"))
            with tempfile.TemporaryFile() as output:
                result = MODULE.run_unix_owned(["owned-child"], ".", output, 10, None)
        return result, signals

    def test_zombie_only_group_is_reaped_without_blind_signal(self):
        result, signals = self.run_fixture([[{"pid": 4242, "status": 5}]])
        self.assertEqual(signals, [0])
        self.assertTrue(result["owned_descendants_settled"])
        self.assertEqual(result["observed_exit"]["status"], 0)
        self.assertEqual(result["exit_code"], 0)
        self.assertFalse(result["owned_group_signaled"])

    def test_denied_census_retains_exit_but_cannot_claim_settlement(self):
        result, signals = self.run_fixture([PermissionError("denied native census")])
        self.assertEqual(signals, [0])
        self.assertFalse(result["owned_descendants_settled"])
        self.assertEqual(result["exit_code"], 0)
        self.assertEqual(result["observed_exit"]["pid"], 4242)
        self.assertIn("denied", result["cleanup_errors"][0]["error"])
        self.assertFalse(MODULE.successful(result))

    def test_live_descendant_is_signaled_only_before_leader_reap(self):
        result, signals = self.run_fixture([[{"pid": 4242, "status": 5}, {"pid": 4243, "status": 2}],
                                           [{"pid": 4242, "status": 5}]])
        self.assertEqual(signals, [MODULE.signal.SIGTERM, 0])
        self.assertTrue(result["owned_descendants_settled"])
        self.assertTrue(result["owned_group_signaled"])
        self.assertEqual(len(result["darwin_group_censuses"]), 2)


if __name__ == "__main__":
    unittest.main()
