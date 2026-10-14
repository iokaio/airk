# SPDX-License-Identifier: Apache-2.0
import base64
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("ci_macos_signing", Path(__file__).parents[1] / "ci-macos-sign.py")
signing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(signing)


class HostedSigningContracts(unittest.TestCase):
    def test_ref_event_and_host_guards_reject_untrusted_context(self):
        with tempfile.TemporaryDirectory() as temporary:
            environment = {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted",
                           "GITHUB_REF": "refs/heads/main", "GITHUB_EVENT_NAME": "workflow_dispatch",
                           "GITHUB_WORKFLOW_REF": "iokaio/airk/.github/workflows/macos-signing.yml@refs/heads/main",
                           "RUNNER_TEMP": temporary}
            with patch.dict(os.environ, environment, clear=True), patch.object(signing.sys, "platform", "darwin"):
                self.assertEqual(signing.runner_directory(), Path(temporary).resolve() / "airk-macos-signing")
                with patch.dict(os.environ, {"GITHUB_WORKFLOW_REF": "iokaio/airk/.github/workflows/github-release.yml@refs/heads/main"}):
                    self.assertEqual(signing.runner_directory(), Path(temporary).resolve() / "airk-macos-signing")
                for key, value in (("GITHUB_REF", "refs/pull/1/merge"),
                                   ("GITHUB_EVENT_NAME", "pull_request"),
                                   ("GITHUB_WORKFLOW_REF", "other/repo/.github/workflows/macos-signing.yml@refs/heads/main"),
                                   ("RUNNER_ENVIRONMENT", "self-hosted"),
                                   ("GITHUB_WORKFLOW_REF", "iokaio/airk/.github/workflows/ci.yml@refs/heads/main")):
                    with self.subTest(key=key), patch.dict(os.environ, {key: value}):
                        with self.assertRaises(ValueError):
                            signing.runner_directory()

    def test_passwords_use_stdin_and_diagnostics_are_redacted(self):
        secret = "fictional-password"
        failed = subprocess.CompletedProcess([], 1, secret, secret)
        with patch.dict(os.environ, {"MACOS_CERTIFICATE_PASSWORD": secret}), \
                patch.object(signing.subprocess, "run", return_value=failed) as run:
            with self.assertRaises(RuntimeError) as error:
                signing.security("create-keychain", "-p", secret, "/tmp/fictional keychain")
            self.assertNotIn(secret, str(error.exception))
            self.assertNotIn(secret, repr(run.call_args.args))
            self.assertIn(secret, run.call_args.kwargs["input"])
            self.assertNotIn("MACOS_CERTIFICATE_PASSWORD", run.call_args.kwargs["env"])
        with patch.object(signing, "private_run") as run:
            with self.assertRaises(ValueError):
                signing.security("import", "bad\ncommand")
            run.assert_not_called()

    def test_failed_notary_credentials_cleanup_and_never_sign(self):
        values = {"MACOS_CERTIFICATE_P12": base64.b64encode(b"fictional p12").decode(),
                  "MACOS_CERTIFICATE_PASSWORD": "fixture", "APPLE_ID": "fixture@example.invalid",
                  "APPLE_APP_SPECIFIC_PASSWORD": "fixture-notary"}
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary) / "credentials"
            def security(*args):
                if args[0] == "create-keychain":
                    Path(args[-1]).touch()
                if args[0] == "find-identity":
                    return signing.IDENTITY
                return ""
            with patch.dict(os.environ, values), patch.object(signing, "runner_directory", return_value=directory), \
                    patch.object(signing, "security", side_effect=security) as security_call, \
                    patch.object(signing, "private_run", side_effect=RuntimeError("notary rejected")), \
                    patch.object(signing.subprocess, "run") as run:
                with self.assertRaisesRegex(RuntimeError, "notary rejected"):
                    signing.sign("aarch64-apple-darwin")
                self.assertFalse(directory.exists())
                self.assertEqual(security_call.call_args.args[0], "delete-keychain")
                run.assert_not_called()
                self.assertFalse(any(name in os.environ for name in signing.SECRET_NAMES))


if __name__ == "__main__":
    unittest.main()
