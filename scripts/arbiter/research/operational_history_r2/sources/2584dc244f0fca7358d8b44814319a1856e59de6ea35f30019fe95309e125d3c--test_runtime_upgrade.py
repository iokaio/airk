# SPDX-License-Identifier: Apache-2.0
"""Upgrade fixture retention and exact cleanup ordering; no native runtime is launched."""
import importlib.util
from pathlib import Path
import tempfile
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
            observed = UPGRADE.finish_fixture(temporary, root, True, lambda: calls.append("uninstall"))
            self.assertEqual(calls, ["uninstall"])
            self.assertTrue(observed["teardown_complete"])
            self.assertFalse(root.exists())
            self.assertTrue(sibling.is_dir())

if __name__ == "__main__": unittest.main()
