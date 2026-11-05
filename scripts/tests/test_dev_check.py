# SPDX-License-Identifier: Apache-2.0
"""Development linting must retain scope, failures and explicit cache choices."""
import contextlib
import importlib.util
import io
import os
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("dev_check", Path(__file__).resolve().parents[1] / "dev-check.py")
DEV = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(DEV)


class DevelopmentChecks(unittest.TestCase):
    def run_main(self, arguments, platform, results):
        observed = []

        def run(command, **options):
            observed.append((command, options))
            return subprocess.CompletedProcess(command, results[len(observed) - 1])

        with patch.object(DEV.sys, "platform", platform), patch.object(DEV.subprocess, "run", run), contextlib.redirect_stdout(io.StringIO()):
            result = DEV.main(arguments)
        return result, observed

    def test_windows_scope_and_explicit_cache_survive_without_changing_parent_environment(self):
        before = os.environ.copy()
        result, calls = self.run_main(["-p", "airk-store", "-p", "airk-core", "--target-dir", "artifacts/dev-cache", "--offline"], "win32", [0, 0])
        self.assertEqual(result, 0)
        command, options = calls[-1]
        self.assertEqual(command.count("-p"), 2)
        self.assertNotIn("--workspace", command)
        self.assertIn("--no-default-features", command)
        self.assertIn("--timings", command)
        self.assertEqual(command[-3:], ["--", "-D", "warnings"])
        self.assertEqual(options["env"]["CARGO_INCREMENTAL"], "1")
        self.assertEqual(Path(options["env"]["CARGO_TARGET_DIR"]), (DEV.ROOT / "artifacts/dev-cache").resolve())
        self.assertEqual(os.environ.copy(), before)

    def test_format_failure_stops_before_clippy_and_keeps_failure_exit_code(self):
        result, calls = self.run_main(["-p", "airk-cli"], "darwin", [7])
        self.assertEqual(result, 7)
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0][1]["env"]["CARGO_INCREMENTAL"], "0")

    def test_explicit_broad_lint_and_incremental_override_preserve_clippy_failure(self):
        result, calls = self.run_main(["--workspace", "--all-features", "--incremental", "off", "--skip-fmt"], "win32", [9])
        self.assertEqual(result, 9)
        self.assertEqual(len(calls), 1)
        self.assertIn("--workspace", calls[0][0])
        self.assertIn("--all-features", calls[0][0])
        self.assertNotIn("--no-default-features", calls[0][0])
        self.assertEqual(calls[0][1]["env"]["CARGO_INCREMENTAL"], "0")

    def test_scope_is_required_and_broad_and_narrow_cannot_be_mixed(self):
        for arguments in ([], ["--workspace", "-p", "airk-store"]):
            with self.subTest(arguments=arguments), contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as error:
                    DEV.main(arguments)
                self.assertEqual(error.exception.code, 2)

    def test_dry_run_with_empty_inherited_target_never_launches_cargo(self):
        output = io.StringIO()
        with patch.dict(os.environ, {"CARGO_TARGET_DIR": ""}), patch.object(DEV.subprocess, "run") as run, contextlib.redirect_stdout(output):
            self.assertEqual(DEV.main(["-p", "airk-core", "--dry-run"]), 0)
        run.assert_not_called()
        self.assertIn(str((DEV.ROOT / "target").resolve()), output.getvalue())

    def test_inherited_cache_and_selected_features_are_forwarded(self):
        with patch.dict(os.environ, {"CARGO_TARGET_DIR": "artifacts/inherited-cache"}):
            result, calls = self.run_main(["-p", "airk-cli", "--features", "local-inference", "--incremental", "on", "--skip-fmt"], "darwin", [0])
        self.assertEqual(result, 0)
        command, options = calls[0]
        self.assertEqual(command[command.index("--features") + 1], "local-inference")
        self.assertEqual(options["env"]["CARGO_INCREMENTAL"], "1")
        self.assertEqual(Path(options["env"]["CARGO_TARGET_DIR"]), (DEV.ROOT / "artifacts/inherited-cache").resolve())


if __name__ == "__main__":
    unittest.main()
