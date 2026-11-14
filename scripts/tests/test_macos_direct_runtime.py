# SPDX-License-Identifier: Apache-2.0
"""Observation guards only; actual Service Management needs the native Mac."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("mac_direct_fixture", ROOT / "scripts/check-macos-direct-runtime.py")
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)


class MacDirectRuntimeTests(unittest.TestCase):
    def setUp(self):
        self.main = Path("/owned fixture/AIRK.app/Contents/MacOS/airk")
        self.plist = self.main.parents[1] / "Library/LaunchAgents/io.iokaio.airk.service.preview.plist"
        self.text = (f"gui/501/{fixture.LABEL} = {{\n\tpath = {self.plist}\n\tprogram = {self.main}\n"
                     "\targuments = {\n\t\tairk\n\t\truntime\n\t\tlogin-serve\n\t\t--channel\n\t\tpreview\n\t}\n"
                     "\tpid = 123\n\tenvironment = {\n\t\tpid = 456\n\t}\n}\n")

    def test_retained_mac_cli_entry_points_cross_the_installer_crate_boundary(self):
        installer=(ROOT/'crates/airk-installer/src/runtime_install.rs').read_text(encoding='utf-8')
        adapter=(ROOT/'crates/airk-installer/src/runtime_macos_login.rs').read_text(encoding='utf-8')
        cli=(ROOT/'crates/airk-cli/src/runtime_commands.rs').read_text(encoding='utf-8')
        exports=(ROOT/'crates/airk-cli/src/lib.rs').read_text(encoding='utf-8')
        self.assertIn('pub mod runtime_install;', (ROOT/'crates/airk-installer/src/lib.rs').read_text(encoding='utf-8'))
        self.assertIn('pub use airk_installer::runtime_install;',exports)
        self.assertRegex(installer,r'pub mod macos_login;')
        # Independent compatibility routes: each surviving external caller must
        # reach a public entry, while implementation details remain private.
        for entry in ('native','foreground_root','direct_root'):
            with self.subTest(entry=entry):
                self.assertIn('macos_login::'+entry+'(',cli)
                self.assertRegex(adapter,r'pub fn '+entry+r'\(')
                self.assertNotIn('pub(crate) fn '+entry+'(',adapter)
        self.assertIn('fn startup_root(',adapter)
        self.assertNotIn('pub fn startup_root(',adapter)
        manager=(ROOT/'crates/airk-installer/src/service_manager.rs').read_text(encoding='utf-8')
        self.assertIn('pub(crate) fn verify_macos_login_intent(',manager)
        self.assertIn('crate::service_manager::verify_macos_login_intent(',adapter)
        native=(ROOT/'crates/airk-installer/src/service_manager_macos.rs').read_text(encoding='utf-8')
        self.assertIn('pub(super) fn verify_intent(',native)

    def test_direct_main_binds_top_level_job_pid(self):
        observed = fixture.job_observation(self.text, self.main, self.plist)
        self.assertEqual(observed["process_id"], 123)
        self.assertEqual(observed["arguments"][2], "login-serve")

    def test_nested_pid_cannot_replace_missing_native_owner(self):
        with self.assertRaises(ValueError):
            fixture.job_observation(self.text.replace("\tpid = 123\n", ""), self.main, self.plist)

    def test_legacy_wrapper_and_foreign_program_are_rejected(self):
        for text in (self.text.replace("login-serve", "login-foreground"),
                     self.text.replace(f"program = {self.main}", "program = /foreign/airk"),
                     self.text.replace(f"path = {self.plist}", "path = /foreign/service.plist")):
            with self.subTest(text=text), self.assertRaises(ValueError):
                fixture.job_observation(text, self.main, self.plist)

    def test_duplicate_and_unbalanced_job_observations_are_rejected(self):
        for text in (self.text.replace("\tpid = 123", "\tpid = 123\n\tpid = 123"),
                     self.text[:-2], "x" * 65537):
            with self.subTest(length=len(text)), self.assertRaises(ValueError):
                fixture.job_observation(text, self.main, self.plist)

    def test_foreign_pending_or_unknown_registration_is_preserved(self):
        clear = {"installed": False, "loaded": False, "running": False, "manager_output": "not_registered"}
        fixture.require_unregistered(clear)
        for changed in ({"installed": True}, {"loaded": True}, {"loaded": None},
                        {"running": True}, {"manager_output": "requires_approval"}):
            with self.subTest(changed=changed), self.assertRaises(RuntimeError):
                fixture.require_unregistered(dict(clear, **changed))

    def test_stop_preserves_owned_registration_without_claiming_foreign_absence(self):
        stopped = {"installed":True,"loaded":False,"running":False,"manager_output":"not_registered"}
        fixture.require_stopped_registration(stopped)
        for changed in ({"installed":False}, {"loaded":None}, {"running":True}, {"manager_output":"requires_approval"}):
            with self.subTest(changed=changed), self.assertRaises(RuntimeError):
                fixture.require_stopped_registration(dict(stopped, **changed))

    def test_selected_image_and_exact_durable_user_stop_are_required(self):
        expected = {"payload_verified":True,"selection":{"active":"a"*64},"executable":str(self.main),
                    "lifecycle":{"explicitly_stopped":True,"pending":None}}
        fixture.require_selected_lifecycle(expected, "a"*64, self.main, True)
        for changed in ({"payload_verified":False}, {"selection":{"active":"b"*64}},
                        {"executable":"/foreign/airk"}, {"lifecycle":{"explicitly_stopped":False,"pending":None}},
                        {"lifecycle":{"explicitly_stopped":True,"pending":{"state":"unknown"}}}):
            with self.subTest(changed=changed), self.assertRaises(RuntimeError):
                fixture.require_selected_lifecycle(dict(expected, **changed), "a"*64, self.main, True)

    def test_exact_pid_exit_refuses_identity_reuse_instead_of_claiming_settlement(self):
        with patch.object(fixture, "process_path", side_effect=[self.main, None]), patch.object(fixture.time, "sleep"):
            fixture.wait_exact_exit(123, self.main)
        with patch.object(fixture, "process_path", return_value=Path("/foreign/airk")), self.assertRaisesRegex(RuntimeError, "exit remains unverified"):
            fixture.wait_exact_exit(123, self.main)


if __name__ == "__main__":
    unittest.main()
