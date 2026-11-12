# SPDX-License-Identifier: Apache-2.0
import base64
import hashlib
import importlib.util
import os
import json
from pathlib import Path
import shlex
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

spec = importlib.util.spec_from_file_location("ci_macos_signing", Path(__file__).parents[1] / "ci-macos-sign.py")
signing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(signing)
builder_spec = importlib.util.spec_from_file_location("signing_runtime_builder", signing.ROOT / "scripts/build-service-runtime.py")
builder = importlib.util.module_from_spec(builder_spec)
builder_spec.loader.exec_module(builder)

PUBLIC_DER = b"fictional public leaf certificate for guard tests"
PUBLIC_SHA256 = hashlib.sha256(PUBLIC_DER).hexdigest()
PUBLIC_SHA1 = hashlib.sha1(PUBLIC_DER).hexdigest()
PUBLIC_PEM = "-----BEGIN CERTIFICATE-----\n" + base64.b64encode(PUBLIC_DER).decode() + "\n-----END CERTIFICATE-----\n"


def valid_identities(*rows):
    return "\n".join(f'  {at}) {sha1.upper()} "{name}"' for at, (sha1, name) in enumerate(rows, 1)) + f"\n  {len(rows)} valid identities found\n"


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

    def test_extension_contracts_run_after_the_build_without_a_native_test_binary(self):
        # Split (U29): the UI-only extension starts no native image, so no step hands
        # it AIRK_TEST_BINARY; the signed pair is probed by ci-macos-sign.py --pair.
        workflow = (signing.ROOT / ".github/workflows/macos-signing.yml").read_text(encoding="utf-8")
        self.assertNotIn("AIRK_TEST_BINARY:", workflow)
        build = workflow.index("python scripts/build-service-runtime.py --profile release")
        suite = workflow.index("run: npm test --prefix packages/vscode")
        transport = workflow.index("uses: ./.github/actions/macos-ticket-network")
        sign = workflow.index("run: python scripts/ci-macos-sign.py --target ${{ matrix.target }} --pair")
        self.assertLess(build, suite)
        self.assertLess(suite, transport)
        self.assertLess(transport, sign)

    def test_signer_can_resolve_chain_and_restores_search_list_on_success_or_failure(self):
        values = {"MACOS_CERTIFICATE_P12": base64.b64encode(b"fictional p12").decode(),
                  "MACOS_CERTIFICATE_PASSWORD": "fixture", "APPLE_ID": "fixture@example.invalid",
                  "APPLE_APP_SPECIFIC_PASSWORD": "fixture-notary"}
        original = ["/Users/runner/Library/Keychains/login.keychain-db", "/tmp/existing keychain"]
        for failure in (None, "notary", "signer", "search-list", "certificate", "execution"):
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
                        return valid_identities((PUBLIC_SHA1, signing.IDENTITY))
                    elif args[0] == "find-certificate":
                        return PUBLIC_PEM
                    return ""

                def notary(*args, **kwargs):
                    if failure == "notary":
                        raise RuntimeError("notary failed")

                def signer(*args, **kwargs):
                    self.assertIn(keychain, search_list)
                    self.assertEqual(search_list[1:], original)
                    self.assertTrue(Path(keychain).exists())
                    self.assertFalse(any(name in os.environ for name in signing.SECRET_NAMES))
                    command = args[0]
                    self.assertEqual(command[command.index("--identity") + 1], PUBLIC_SHA1)
                    if failure == "signer":
                        raise RuntimeError("signer failed")

                def execution_probe(*args):
                    self.assertFalse(directory.exists())
                    self.assertEqual(search_list, original)
                    self.assertFalse(any(name in os.environ for name in signing.SECRET_NAMES))
                    self.assertEqual(args[1:], (True, "aarch64-apple-darwin"))
                    if failure == "execution":
                        raise RuntimeError("execution failed")

                with patch.dict(os.environ, values), \
                        patch.object(signing, "CERTIFICATE_SHA256", PUBLIC_SHA256), \
                        patch.object(signing, "runner_directory", return_value=directory), \
                        patch.object(signing, "security", side_effect=security), \
                        patch.object(signing, "private_run", side_effect=notary), \
                        patch.object(signing.subprocess, "run", side_effect=signer) as run, \
                        patch.object(signing, "verify_signed_output", side_effect=RuntimeError("certificate failed") if failure == "certificate" else None) as audit, \
                        patch.object(signing, "qualify_signed_runtime", side_effect=execution_probe) as probe:
                    if failure:
                        with self.assertRaisesRegex(RuntimeError, failure + " failed"):
                            signing.sign("aarch64-apple-darwin", paired=True)
                    else:
                        signing.sign("aarch64-apple-darwin", paired=True)
                    self.assertEqual(run.call_count, int(failure in (None, "signer", "certificate", "execution")))
                    self.assertEqual(audit.call_count, int(failure in (None, "certificate", "execution")))
                    self.assertEqual(probe.call_count, int(failure in (None, "execution")))
                    self.assertEqual(search_list, original)
                    self.assertFalse(directory.exists())

    def test_signed_runtime_probes_exact_bundle_and_fail_closed_without_payload_mutation(self):
        for failure in (None, "model", "install", "payload"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary)
                runtime = output / "runtime"
                main = runtime / "AIRK.app/Contents/MacOS/airk"
                main.parent.mkdir(parents=True)
                main.write_bytes(b"inert signed-bundle fixture, never executed")
                files = {"AIRK.app/Contents/MacOS/airk":hashlib.sha256(main.read_bytes()).hexdigest()}
                builder = SimpleNamespace(single_image_version=Mock(return_value=True), source_identity=Mock(return_value="input seal"),
                    verify_model_smoke=Mock(side_effect=RuntimeError("model refused") if failure == "model" else None,
                                            return_value={"source":"embedded","pipeline_digest":"a" * 64}))
                package = SimpleNamespace(bounded_json=Mock(return_value={"schema":2,"version":"1.8.14"}),
                    runtime_builder=Mock(return_value=builder), validate_runtime=Mock(return_value=(files,tuple(files))),
                    sha256=Mock(return_value="b" * 64), inventory=Mock(return_value={} if failure == "payload" else files))
                spec = SimpleNamespace(loader=SimpleNamespace(exec_module=lambda value:None))
                token = "airk1." + "c" * 32 + "." + "d" * 64
                stderr = '  File "/private/owned/check-service-install.py", line 123, in main\nRuntimeError: native_install_failed ' + token
                completed = subprocess.CompletedProcess([], 1 if failure == "install" else 0, token, stderr)
                with patch.object(signing.importlib.util, "spec_from_file_location", return_value=spec), \
                        patch.object(signing.importlib.util, "module_from_spec", return_value=package), \
                        patch.object(signing.subprocess, "run", return_value=completed) as execute, patch("builtins.print"):
                    if failure:
                        with self.assertRaises(RuntimeError):
                            signing.qualify_signed_runtime(output, True, "aarch64-apple-darwin")
                    else:
                        signing.qualify_signed_runtime(output, True, "aarch64-apple-darwin")
                package.validate_runtime.assert_called_once_with(runtime,"1.8.14","aarch64-apple-darwin","input seal")
                builder.verify_model_smoke.assert_called_once_with(runtime,"macos","1.8.14")
                if failure == "model":
                    execute.assert_not_called()
                else:
                    self.assertEqual(execute.call_args.args[0][-2:], ["--runtime",str(runtime)])
                    self.assertFalse(set(signing.SECRET_NAMES) & execute.call_args.kwargs["env"].keys())
                    self.assertNotIn("timeout", execute.call_args.kwargs)
                receipt = (output / "post-sign-validation.json").read_text()
                self.assertEqual(json.loads(receipt)["status"], "failed" if failure else "passed")
                self.assertNotIn(token, receipt)
                if failure != "model":
                    self.assertEqual(json.loads(receipt)["service_install"]["diagnostics"], {
                        "exception_types":["RuntimeError"], "native_error_codes":["native_install_failed"],
                        "frames":[{"file":"check-service-install.py","line":123,"function":"main"}]})
                self.assertEqual(main.read_bytes(), b"inert signed-bundle fixture, never executed")

    def test_post_sign_execution_refuses_remaining_signing_secrets(self):
        with patch.dict(os.environ, {"MACOS_CERTIFICATE_PASSWORD":"fictional secret"}), \
                patch.object(signing.subprocess, "run") as execute:
            with self.assertRaisesRegex(ValueError, "secrets"):
                signing.qualify_signed_runtime(Path("unused"), True, "aarch64-apple-darwin")
        execute.assert_not_called()

    def test_modern_signed_schema_cannot_downgrade_to_skip_execution_probes(self):
        builder = SimpleNamespace(single_image_version=Mock(return_value=True))
        package = SimpleNamespace(bounded_json=Mock(return_value={"schema":1,"version":"1.8.14"}),
                                  runtime_builder=Mock(return_value=builder))
        spec = SimpleNamespace(loader=SimpleNamespace(exec_module=lambda value:None))
        with patch.object(signing.importlib.util, "spec_from_file_location", return_value=spec), \
                patch.object(signing.importlib.util, "module_from_spec", return_value=package), \
                patch.object(signing.subprocess, "run") as execute:
            with self.assertRaisesRegex(ValueError, "schema"):
                signing.qualify_signed_runtime(Path("unused"), True, "aarch64-apple-darwin")
        execute.assert_not_called()

    def split_probe_fixture(self, output, schema=3, version="1.8.24"):
        runtime = output / "runtime"
        names = ("AIRK.app/Contents/MacOS/airk", "AIRK.app/Contents/MacOS/airkctl")
        for name in names:
            (runtime / name).parent.mkdir(parents=True, exist_ok=True)
            (runtime / name).write_bytes(b"inert signed split image " + name.encode())
        files = {name: hashlib.sha256((runtime / name).read_bytes()).hexdigest() for name in names}
        builder = SimpleNamespace(single_image_version=Mock(return_value=True), source_identity=Mock(return_value="input seal"),
                                  verify_model_smoke=Mock(return_value={"source": "embedded", "pipeline_digest": "a" * 64}))
        package = SimpleNamespace(bounded_json=Mock(return_value={"schema": schema, "version": version}),
                                  runtime_builder=Mock(return_value=builder), validate_runtime=Mock(return_value=(files, names)),
                                  sha256=Mock(return_value="b" * 64), inventory=Mock(return_value=files))
        spec = SimpleNamespace(loader=SimpleNamespace(exec_module=lambda value: None))
        return runtime, builder, package, spec

    def test_split_signed_pair_probes_model_and_nested_airkctl_but_not_the_presplit_install_harness(self):
        for reply in ("airkctl 1.8.24", "airkctl 1.8.23"):
            with self.subTest(reply=reply), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary)
                runtime, builder, package, spec = self.split_probe_fixture(output)
                completed = subprocess.CompletedProcess([], 0, reply + "\n", "")
                with patch.object(signing.importlib.util, "spec_from_file_location", return_value=spec), \
                        patch.object(signing.importlib.util, "module_from_spec", return_value=package), \
                        patch.object(signing.subprocess, "run", return_value=completed) as execute, patch("builtins.print"):
                    if reply.endswith("1.8.24"):
                        signing.qualify_signed_runtime(output, True, "aarch64-apple-darwin")
                    else:
                        with self.assertRaisesRegex(RuntimeError, "airkctl"):
                            signing.qualify_signed_runtime(output, True, "aarch64-apple-darwin")
                builder.verify_model_smoke.assert_called_once_with(runtime, "macos", "1.8.24")
                self.assertEqual(execute.call_count, 1)
                self.assertEqual(execute.call_args.args[0], [str(runtime / "AIRK.app/Contents/MacOS/airkctl"), "--version"])
                self.assertFalse(set(signing.SECRET_NAMES) & execute.call_args.kwargs["env"].keys())
                report = json.loads((output / "post-sign-validation.json").read_text())
                self.assertNotIn("service_install", report)
                if reply.endswith("1.8.24"):
                    self.assertEqual(report["status"], "passed")
                    self.assertEqual(report["stages"], ["signed_embedded_model_passed", "signed_airkctl_version_passed"])
                else:
                    self.assertEqual((report["status"], report["failure_stage"]), ("failed", "signed_airkctl_version"))

    def test_split_version_requires_the_schema_3_pair_descriptor(self):
        with tempfile.TemporaryDirectory() as temporary:
            _, _, package, spec = self.split_probe_fixture(Path(temporary), schema=2)
            with patch.object(signing.importlib.util, "spec_from_file_location", return_value=spec), \
                    patch.object(signing.importlib.util, "module_from_spec", return_value=package), \
                    patch.object(signing.subprocess, "run") as execute:
                with self.assertRaisesRegex(ValueError, "schema"):
                    signing.qualify_signed_runtime(Path(temporary), True, "aarch64-apple-darwin")
            execute.assert_not_called()

    def test_certificate_audit_covers_the_split_nested_airkctl(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            names = ["AIRK.app/Contents/MacOS/airk", "AIRK.app/Contents/MacOS/airkctl"]
            for name in names:
                (output / "runtime" / name).parent.mkdir(parents=True, exist_ok=True)
                (output / "runtime" / name).write_bytes(b"fictional signed bytes")
            (output / "signing.json").write_text(json.dumps({"schema": "airk-service-pair-signing/1",
                                                             "components": {name: {} for name in names}}), encoding="utf-8")
            with patch.object(signing, "verify_signed_certificate") as verify, patch("builtins.print"):
                audit = signing.verify_signed_output(output, True, PUBLIC_SHA1)
            self.assertEqual(audit["signed_paths"], ["runtime/AIRK.app/Contents/MacOS/airk",
                                                     "runtime/AIRK.app/Contents/MacOS/airkctl", "runtime/AIRK.app"])
            self.assertEqual(verify.call_count, 3)

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
                    return valid_identities((PUBLIC_SHA1, signing.IDENTITY))
                if args[0] == "find-certificate":
                    return PUBLIC_PEM
                return ""
            with patch.dict(os.environ, values), patch.object(signing, "runner_directory", return_value=directory), \
                    patch.object(signing, "CERTIFICATE_SHA256", PUBLIC_SHA256), \
                    patch.object(signing, "security", side_effect=security) as security_call, \
                    patch.object(signing, "private_run", side_effect=RuntimeError("notary rejected")), \
                    patch.object(signing.subprocess, "run") as run:
                with self.assertRaisesRegex(RuntimeError, "notary rejected"):
                    signing.sign("aarch64-apple-darwin")
                self.assertFalse(directory.exists())
                self.assertEqual(security_call.call_args.args[0], "delete-keychain")
                run.assert_not_called()
                self.assertFalse(any(name in os.environ for name in signing.SECRET_NAMES))

    def test_imported_public_pin_requires_the_matching_unique_private_key_identity(self):
        good = valid_identities((PUBLIC_SHA1, signing.IDENTITY))
        cases = [
            (PUBLIC_PEM, good, None),
            (PUBLIC_PEM, valid_identities((PUBLIC_SHA1, signing.IDENTITY), ("b" * 40, "Other signing identity")), None),
            ("", good, "missing or ambiguous"),
            (PUBLIC_PEM + PUBLIC_PEM, good, "missing or ambiguous"),
            (PUBLIC_PEM.replace(base64.b64encode(PUBLIC_DER).decode(), base64.b64encode(b"other public certificate").decode()), good, "SHA-256 pin"),
            (PUBLIC_PEM, valid_identities(), "unambiguous valid"),
            (PUBLIC_PEM, valid_identities(("b" * 40, signing.IDENTITY)), "unambiguous valid"),
            (PUBLIC_PEM, valid_identities((PUBLIC_SHA1, signing.IDENTITY), ("b" * 40, signing.IDENTITY)), "unambiguous valid"),
            (PUBLIC_PEM, valid_identities((PUBLIC_SHA1, "Other signing identity")), "unambiguous valid"),
            (PUBLIC_PEM, good.replace("1 valid", "2 valid"), "unambiguous valid"),
        ]
        for pem, identities, error in cases:
            with self.subTest(error=error, identities=identities), patch.object(signing, "CERTIFICATE_SHA256", PUBLIC_SHA256), \
                    patch.object(signing, "security", side_effect=[pem, identities]) as command:
                if error:
                    with self.assertRaisesRegex(ValueError, error):
                        signing.imported_identity(Path("fictional.keychain-db"))
                else:
                    self.assertEqual(signing.imported_identity(Path("fictional.keychain-db")), PUBLIC_SHA1)
                self.assertTrue(all(call.args[0] in {"find-certificate", "find-identity"} for call in command.call_args_list),
                                "the guard reads public certificates and identity metadata, never exports private keys")

    def test_actual_signed_leaf_must_match_the_public_sha256_pin(self):
        prefixes = []
        for der, expected_error in ((PUBLIC_DER, None), (b"another actual signing leaf", "differs from")):
            with self.subTest(expected_error=expected_error):
                def extract(command, **kwargs):
                    self.assertEqual(command[:2], ["/usr/bin/codesign", "--display"])
                    self.assertEqual(len(command), 4)
                    self.assertTrue(command[2].startswith("--extract-certificates="))
                    prefix = Path(command[2].split("=", 1)[1]); prefixes.append(prefix)
                    Path(str(prefix) + "0").write_bytes(der)
                with patch.object(signing, "CERTIFICATE_SHA256", PUBLIC_SHA256), patch.object(signing, "private_run", side_effect=extract):
                    if expected_error:
                        with self.assertRaisesRegex(ValueError, expected_error):
                            signing.verify_signed_certificate(Path("fictional signed binary"), PUBLIC_SHA1)
                    else:
                        signing.verify_signed_certificate(Path("fictional signed binary"), PUBLIC_SHA1)
        self.assertTrue(all(not prefix.parent.exists() for prefix in prefixes))

    def test_wrong_or_ambiguous_import_cleans_up_before_notary_or_signing(self):
        values = {"MACOS_CERTIFICATE_P12": base64.b64encode(b"fictional p12").decode(),
                  "MACOS_CERTIFICATE_PASSWORD": "fixture-password", "APPLE_ID": "fixture@example.invalid",
                  "APPLE_APP_SPECIFIC_PASSWORD": "fixture-notary-password"}
        for ambiguous in (False, True):
            with self.subTest(ambiguous=ambiguous), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary) / "credentials"
                original = ["/tmp/original.keychain-db"]
                search = original.copy()
                def security(*args):
                    if args[0] == "create-keychain":
                        Path(args[-1]).touch()
                    if args[0] == "list-keychains":
                        if "-s" not in args:
                            return shlex.join(search)
                        search[:] = map(str, args[4:])
                    if args[0] == "find-certificate":
                        return PUBLIC_PEM if ambiguous else PUBLIC_PEM.replace(base64.b64encode(PUBLIC_DER).decode(), base64.b64encode(b"wrong public leaf").decode())
                    if args[0] == "find-identity":
                        return valid_identities((PUBLIC_SHA1, signing.IDENTITY), ("b" * 40, signing.IDENTITY))
                    return ""
                with patch.dict(os.environ, values), patch.object(signing, "runner_directory", return_value=directory), \
                        patch.object(signing, "CERTIFICATE_SHA256", PUBLIC_SHA256), patch.object(signing, "security", side_effect=security), \
                        patch.object(signing, "private_run") as notary, patch.object(signing.subprocess, "run") as signer:
                    with self.assertRaises(ValueError) as failure:
                        signing.sign("aarch64-apple-darwin", paired=True)
                    self.assertTrue(all(value not in str(failure.exception) for value in values.values()))
                    notary.assert_not_called(); signer.assert_not_called()
                    self.assertEqual(search, original)
                    self.assertFalse(directory.exists())

    def test_public_certificate_audit_checks_the_app_and_all_components_without_changing_payload(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            names = ["AIRK.app/Contents/MacOS/" + name for name in ("airk", "airk-embed", "airk-launcher-image", "libonnxruntime.dylib")]
            for name in names:
                path = output / "runtime" / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"fictional immutable signed bytes")
            receipt = {"schema": "airk-service-pair-signing/1", "components": {name: {} for name in names}}
            (output / "signing.json").write_text(json.dumps(receipt), encoding="utf-8")
            before = {path.relative_to(output): path.read_bytes() for path in output.rglob("*") if path.is_file()}
            with patch.object(signing, "CERTIFICATE_SHA256", PUBLIC_SHA256), \
                    patch.object(signing, "verify_signed_certificate") as verify, patch("builtins.print") as log:
                audit = signing.verify_signed_output(output, True, PUBLIC_SHA1)
                self.assertEqual(len(verify.call_args_list), 5)
                self.assertEqual(verify.call_args_list[-1].args, (output / "runtime/AIRK.app", PUBLIC_SHA1))
                self.assertNotIn("MACOS_CERTIFICATE", str(log.call_args))
            self.assertEqual(audit["certificate_sha256"], PUBLIC_SHA256)
            self.assertTrue((output / "certificate-identity.json").is_file())
            self.assertEqual(before, {name: (output / name).read_bytes() for name in before})
            receipt["components"] = {"../outside": {}}
            (output / "signing.json").write_text(json.dumps(receipt), encoding="utf-8")
            with patch.object(signing, "verify_signed_certificate") as verify, self.assertRaisesRegex(ValueError, "audit component"):
                signing.verify_signed_output(output, True, PUBLIC_SHA1)
            verify.assert_not_called()


if __name__ == "__main__":
    unittest.main()
