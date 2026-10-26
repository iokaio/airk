# SPDX-License-Identifier: Apache-2.0
import base64
import importlib.util
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("ci_macos_signing", Path(__file__).parents[1] / "ci-macos-sign.py")
signing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(signing)
builder_spec = importlib.util.spec_from_file_location("signing_runtime_builder", signing.ROOT / "scripts/build-service-runtime.py")
builder = importlib.util.module_from_spec(builder_spec)
builder_spec.loader.exec_module(builder)


class HostedSigningContracts(unittest.TestCase):
    def test_shared_ticket_transport_covers_every_hosted_mac_signing_caller(self):
        action_path = ".github/actions/macos-ticket-network/action.yml"
        action = (signing.ROOT / action_path).read_text(encoding="utf-8")
        self.assertIn("using: composite", action)
        self.assertIn("shell: bash", action)
        self.assertEqual([line.strip() for line in action.split("run: |\n", 1)[1].splitlines()
                          if line.strip() and not line.lstrip().startswith("#")], [
            'echo "block return out quick proto udp from any to any port 443" | sudo pfctl -f -',
            "sudo pfctl -E",
        ])
        callers = []
        for path in (signing.ROOT / ".github/workflows").glob("*.yml"):
            workflow = path.read_text(encoding="utf-8")
            steps = workflow.split("\n      - ")
            for index, step in enumerate(steps):
                if "ci-macos-sign.py" not in step or "--cleanup" in step:
                    continue
                callers.append(path.name)
                self.assertIn("x86_64-apple-darwin", workflow)
                self.assertIn("aarch64-apple-darwin", workflow)
                self.assertGreater(index, 0)
                transport = steps[index - 1]
                self.assertIn("uses: ./.github/actions/macos-ticket-network", transport)
                guard = [line.strip() for line in transport.splitlines() if line.lstrip().startswith("if:")]
                self.assertEqual(guard, ["if: runner.os == 'macOS'"] if path.name == "github-release.yml" else [])
                self.assertIn("run: python scripts/ci-macos-sign.py --target ${{ matrix.target }} --pair", step)
                self.assertNotIn("continue-on-error", step)
                self.assertNotIn("||", step)
        self.assertCountEqual(callers, ["github-release.yml", "macos-signing.yml"])

    def test_standalone_signing_smoke_uses_the_built_mac_bundle_executable(self):
        workflow = (signing.ROOT / ".github/workflows/macos-signing.yml").read_text(encoding="utf-8")
        executable = builder.runtime_names("macos")[0]
        paths = [line.strip() for line in workflow.splitlines() if "AIRK_TEST_BINARY:" in line]
        self.assertEqual(paths, ["AIRK_TEST_BINARY: ${{ github.workspace }}/artifacts/runtime/${{ matrix.target }}/" + executable])
        build = workflow.index("python scripts/build-service-runtime.py --profile release")
        smoke = workflow.index(paths[0])
        transport = workflow.index("uses: ./.github/actions/macos-ticket-network")
        self.assertLess(build, smoke)
        self.assertLess(smoke, transport)

    def test_signer_can_resolve_chain_and_restores_search_list_on_success_or_failure(self):
        values = {"MACOS_CERTIFICATE_P12": base64.b64encode(b"fictional p12").decode(),
                  "MACOS_CERTIFICATE_PASSWORD": "fixture", "APPLE_ID": "fixture@example.invalid",
                  "APPLE_APP_SPECIFIC_PASSWORD": "fixture-notary"}
        original = ["/Users/runner/Library/Keychains/login.keychain-db", "/tmp/existing keychain"]
        for failure in (None, "notary", "signer", "search-list"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary) / "credentials"
                keychain = str(directory / "signing.keychain-db")
                search_list = original.copy()

                def security(*args):
                    if args[0] == "create-keychain":
                        Path(args[-1]).touch()
                    elif args[0] == "list-keychains":
                        if "-s" not in args:
                            return shlex.join(search_list)
                        search_list[:] = map(str, args[4:])
                        if failure == "search-list" and keychain in search_list:
                            raise RuntimeError("search-list failed")
                    elif args[0] == "find-identity":
                        return signing.IDENTITY
                    return ""

                def notary(*args, **kwargs):
                    if failure == "notary":
                        raise RuntimeError("notary failed")

                def signer(*args, **kwargs):
                    self.assertIn(keychain, search_list)
                    self.assertEqual(search_list[1:], original)
                    self.assertTrue(Path(keychain).exists())
                    self.assertFalse(any(name in os.environ for name in signing.SECRET_NAMES))
                    if failure == "signer":
                        raise RuntimeError("signer failed")

                with patch.dict(os.environ, values), \
                        patch.object(signing, "runner_directory", return_value=directory), \
                        patch.object(signing, "security", side_effect=security), \
                        patch.object(signing, "private_run", side_effect=notary), \
                        patch.object(signing.subprocess, "run", side_effect=signer) as run:
                    if failure:
                        with self.assertRaisesRegex(RuntimeError, failure + " failed"):
                            signing.sign("aarch64-apple-darwin")
                    else:
                        signing.sign("aarch64-apple-darwin")
                    self.assertEqual(run.call_count, int(failure in (None, "signer")))
                    self.assertEqual(search_list, original)
                    self.assertFalse(directory.exists())

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
