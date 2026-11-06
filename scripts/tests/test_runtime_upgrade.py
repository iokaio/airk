# SPDX-License-Identifier: Apache-2.0
"""Upgrade fixture retention and exact cleanup ordering; no native runtime is launched."""
import importlib.util
from pathlib import Path
import tempfile
import subprocess
from unittest import mock
import unittest

SPEC = importlib.util.spec_from_file_location("runtime_upgrade", Path(__file__).parents[1] / "check-runtime-upgrade.py")
UPGRADE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(UPGRADE)

class UpgradeFixtureOwnershipTests(unittest.TestCase):
    def test_failure_cleanup_never_masks_original_refusal_or_deletes_fixture(self):
        with tempfile.TemporaryDirectory() as parent:
            temporary = UPGRADE.owned_temporary(Path(parent))
            root = Path(temporary.name)
            (root / "install").mkdir()
            sentinel = root / "diagnostic.json"
            sentinel.write_text('{"native":"original-refusal"}')
            def failed_stop():
                raise RuntimeError("secondary stop failure")
            with self.assertRaisesRegex(RuntimeError, "original ConfigurationUnavailable"):
                try:
                    raise RuntimeError("original ConfigurationUnavailable")
                finally:
                    observed = UPGRADE.finish_fixture(temporary, root, False,
                        lambda: self.fail("failed fixture must not uninstall"), observe_stop=failed_stop)
            self.assertTrue(observed["fixture_retained"])
            self.assertEqual(observed["errors"][0]["operation"], "observe_owned_candidate_stop")
            self.assertEqual(sentinel.read_text(), '{"native":"original-refusal"}')

    def test_pending_committed_activation_never_auto_uninstalls_or_aborts(self):
        with tempfile.TemporaryDirectory() as parent:
            temporary = UPGRADE.owned_temporary(Path(parent)); root = Path(temporary.name)
            install = root / "install"; install.mkdir()
            (install / "activation.json").write_text('{"phase":"probe"}')
            observed = UPGRADE.finish_fixture(temporary, root, True,
                lambda: self.fail("pending activation must not uninstall"))
            self.assertTrue(observed["activation_pending"])
            self.assertTrue(observed["fixture_retained"])
            self.assertFalse(observed["teardown_complete"])
            self.assertTrue((install / "activation.json").is_file())

    def test_uninstall_failure_retains_previously_completed_assertion_evidence(self):
        with tempfile.TemporaryDirectory() as parent:
            temporary = UPGRADE.owned_temporary(Path(parent)); root = Path(temporary.name)
            (root / "install").mkdir()
            def refuse_uninstall(): raise RuntimeError("teardown refused")
            observed = UPGRADE.finish_fixture(temporary, root, True, refuse_uninstall)
            self.assertTrue(root.is_dir())
            self.assertFalse(observed["teardown_complete"])
            self.assertEqual(observed["errors"][0]["operation"], "uninstall")

    def test_success_removes_only_owned_fixture_after_acknowledged_uninstall(self):
        with tempfile.TemporaryDirectory() as parent:
            temporary = UPGRADE.owned_temporary(Path(parent)); root = Path(temporary.name)
            (root / "install").mkdir(); sibling = Path(parent) / "unrelated"; sibling.mkdir()
            calls = []
            observed = UPGRADE.finish_fixture(temporary, root, True, lambda: (calls.append("uninstall") or {"uninstalled": True}))
            self.assertEqual(calls, ["uninstall"])
            self.assertTrue(observed["teardown_complete"])
            self.assertFalse(root.exists())
            self.assertTrue(sibling.is_dir())

    def test_missing_install_never_counts_as_acknowledged_uninstall(self):
        with tempfile.TemporaryDirectory() as parent:
            temporary = UPGRADE.owned_temporary(Path(parent)); root = Path(temporary.name)
            observed = UPGRADE.finish_fixture(temporary, root, True,
                lambda: self.fail("missing install cannot be uninstalled"))
            self.assertFalse(observed["uninstall_acknowledged"])
            self.assertFalse(observed["teardown_complete"])
            self.assertTrue(root.is_dir())
            self.assertIn("missing", observed["errors"][0]["error"])

    def test_deferred_uninstall_preserves_fixture_and_its_unknown_state(self):
        with tempfile.TemporaryDirectory() as parent:
            temporary = UPGRADE.owned_temporary(Path(parent)); root = Path(temporary.name)
            (root / "install").mkdir()
            observed = UPGRADE.finish_fixture(temporary, root, True,
                lambda: {"uninstalled": False, "deferred_runtimes": ["owned"]})
            self.assertFalse(observed["uninstall_acknowledged"])
            self.assertTrue(root.is_dir())

    def test_native_timeout_retains_exact_partial_streams_and_original_exception(self):
        with tempfile.TemporaryDirectory() as parent:
            root = Path(parent); records = []
            error = subprocess.TimeoutExpired(["owned.exe", "runtime", "activate"], 1,
                output=b"partial native stdout", stderr=b"original activation diagnostics")
            with mock.patch.object(UPGRADE.subprocess, "run", side_effect=error):
                with self.assertRaises(subprocess.TimeoutExpired) as observed:
                    UPGRADE.record_native_command(root, records, "owned.exe", ["runtime", "activate"], timeout=1)
            self.assertIs(observed.exception, error)
            self.assertEqual((root / "command-001.stdout").read_bytes(), b"partial native stdout")
            self.assertEqual((root / "command-001.stderr").read_bytes(), b"original activation diagnostics")
            self.assertEqual(records[0]["failure_type"], "TimeoutExpired")
            self.assertIsNone(records[0]["exit_code"])

if __name__ == "__main__": unittest.main()
