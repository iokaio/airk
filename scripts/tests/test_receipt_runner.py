# SPDX-License-Identifier: Apache-2.0
"""The receipt runner must record consumed inputs and refuse empty test selections."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("receipt_runner", ROOT / "scripts" / "dev" / "receipt_runner.py")
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)


class SelectedTests(unittest.TestCase):
    def test_recognizes_runner_summaries(self):
        self.assertEqual(RUNNER.selected_tests("    Starting 5 tests across 1 binary"), 5)
        self.assertEqual(RUNNER.selected_tests("# tests 580\n# pass 576"), 580)
        self.assertEqual(RUNNER.selected_tests("Ran 12 tests in 0.5s"), 12)
        self.assertEqual(RUNNER.selected_tests("running 0 tests\nrunning 3 tests"), 3)
        self.assertIsNone(RUNNER.selected_tests("Finished dev profile"))

    def test_identifies_test_commands(self):
        self.assertTrue(RUNNER.is_test_command(["cargo", "nextest", "run", "-p", "airk-core"]))
        self.assertTrue(RUNNER.is_test_command(["cargo", "test", "-p", "airk-core"]))
        self.assertTrue(RUNNER.is_test_command([r"C:\tools\cargo.exe", "nextest", "run"]))
        self.assertTrue(RUNNER.is_test_command(["node", "--test", "out/test/a.test.js"]))
        self.assertTrue(RUNNER.is_test_command(["python", "-m", "unittest", "discover"]))
        self.assertFalse(RUNNER.is_test_command(["python", "scripts/dev-check.py", "-p", "airk-core"]))

    def test_rejects_bad_names_and_missing_commands(self):
        for argv in (["Bad_Name", "echo"], ["ok-name"]):
            with self.assertRaises(SystemExit), contextlib.redirect_stderr(io.StringIO()):
                RUNNER.parse(argv)


class Receipts(unittest.TestCase):
    def run_runner(self, out, *command, extra=()):
        argv = [sys.executable, str(ROOT / "scripts/dev/receipt_runner.py"), "--groups", "fixtures",
                "--out", str(out), *extra, *command]
        return subprocess.run(argv, cwd=ROOT, capture_output=True, text=True, timeout=300)

    def test_writes_receipt_and_log(self):
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            done = self.run_runner(out, "plain-run", sys.executable, "-c", "print('hello')")
            self.assertEqual(done.returncode, 0, done.stdout + done.stderr)
            receipt = json.loads((out / "plain-run.json").read_text(encoding="utf-8"))
            self.assertEqual(receipt["schema"], RUNNER.SCHEMA)
            self.assertTrue(receipt["inputs_unchanged"])
            self.assertIn("fixtures", receipt["groups"])
            self.assertIn("hello", (out / "plain-run.log").read_text(encoding="utf-8"))
            self.assertTrue((out / "plain-run-runner.py").is_file())

    def test_empty_test_selection_fails(self):
        # A successful test command that reports zero selected tests (the trailing
        # "unittest" argument marks it as a test command).
        empty = (sys.executable, "-c", "print('Ran 0 tests in 0.001s')", "unittest")
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            done = self.run_runner(out, "empty-selection", *empty)
            self.assertEqual(done.returncode, 4, done.stdout + done.stderr)
            self.assertEqual(json.loads((out / "empty-selection.json").read_text())["selected_tests"], 0)
            allowed = self.run_runner(out, "empty-allowed", *empty, extra=("--allow-no-tests",))
            self.assertEqual(allowed.returncode, 0, allowed.stdout + allowed.stderr)

    def test_command_failure_is_propagated(self):
        with tempfile.TemporaryDirectory() as temporary:
            done = self.run_runner(Path(temporary), "failing", sys.executable, "-c", "raise SystemExit(7)")
            self.assertEqual(done.returncode, 7)


if __name__ == "__main__":
    unittest.main()
