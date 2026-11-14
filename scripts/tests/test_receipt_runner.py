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
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("receipt_runner", ROOT / "scripts" / "dev" / "receipt_runner.py")
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)


class SelectedTests(unittest.TestCase):
    def test_nextest_failure_output_does_not_add_captured_child_selections(self):
        log = ("    Starting 4 tests across 2 binaries (81 tests skipped)\n"
               "  stdout ---\n    running 1 test\n    running 1 test\n"
               "    running 1 test\n"
               "     Summary [0.55s] 4 tests run: 1 passed, 3 failed\n")
        self.assertEqual(RUNNER.selected_tests(log), 4)

    def test_mixed_runner_blocks_keep_their_own_selections(self):
        log = ("running 2 tests\n"
               "    Starting 4 tests across 2 binaries\n"
               "    running 1 test\n"
               "     Summary [0.55s] 4 tests run: 1 passed, 3 failed\n"
               "Ran 3 tests in 0.01s\n# tests 5\n")
        self.assertEqual(RUNNER.selected_tests(log), 14)

    def test_node_spec_reporter_records_its_selection(self):
        self.assertEqual(RUNNER.selected_tests("ℹ tests 99\nℹ pass 87\nℹ fail 12\n"), 99)

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
    def test_child_tempfile_uses_owned_base_instead_of_inherited_tmpdir(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary).resolve()
            foreign = base/'foreign'
            foreign.mkdir()
            private = base/'owned-fixtures'
            args = RUNNER.parse(['--fixture-base',str(private),'--groups','',
                                 'temp-choice',sys.executable,'-c','pass'])
            with patch.dict(RUNNER.os.environ,{'TMPDIR':str(foreign)}):
                environment = RUNNER.environment(args,base/'receipts')
            child = ('import tempfile; from pathlib import Path; '
                     'owner=tempfile.TemporaryDirectory(); '
                     'print(Path(owner.name).resolve().parent); owner.cleanup()')
            actual = subprocess.run([sys.executable,'-I','-B','-c',child],env=environment,
                                    capture_output=True,text=True,check=True,timeout=15)
            self.assertEqual(Path(actual.stdout.strip()),private)
            for name in ('TEMP','TMP','TMPDIR'):
                self.assertEqual(environment[name],str(private))

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

    def test_junit_requires_nextest_in_this_commands_log(self):
        for name, summary, expected in (
            ("python-report", "Ran 1 tests in 0.001s", False),
            ("libtest-report", "running 1 test", False),
            ("nextest-report", "Starting 1 test across 1 binary", True),
        ):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                out = Path(temporary)
                target = out / "target"
                junit = target / "nextest/ci/junit.xml"
                # A concurrent Cargo owner can write a fresh report during an
                # unrelated Python check. Freshness alone proves no provenance.
                script = ("from pathlib import Path; import sys; "
                          "p=Path(sys.argv[1]); p.parent.mkdir(parents=True); "
                          "p.write_text('<testsuites/>'); print(sys.argv[2])")
                done = self.run_runner(out, name, sys.executable, "-c", script, str(junit), summary,
                                       extra=("--groups", "", "--target-dir", str(target)))
                self.assertEqual(done.returncode, 0, done.stdout + done.stderr)
                receipt = json.loads((out / (name + ".json")).read_text())
                self.assertEqual("junit" in receipt, expected)
                self.assertEqual((out / (name + "-junit.xml")).exists(), expected)
                if expected:
                    self.assertEqual(receipt["junit"]["sha256"], RUNNER.digest(junit))


if __name__ == "__main__":
    unittest.main()
