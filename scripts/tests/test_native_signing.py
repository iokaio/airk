# SPDX-License-Identifier: Apache-2.0
import importlib.util
import json
from pathlib import Path
import sys
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

spec = importlib.util.spec_from_file_location("native_signing",Path(__file__).parents[1]/"sign-native.py")
signing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(signing)
package_options = signing.pair_package()


class SigningContracts(unittest.TestCase):
    def test_copied_launcher_is_verified_before_execution_and_keeps_exact_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            runtime = Path(temporary)
            main = runtime / "AIRK.app/Contents/MacOS/airk-launcher-image"
            main.parent.mkdir(parents=True)
            main.write_bytes(b"fictional sealed executable")
            component = {"binary_sha256": signing.digest(main)}
            verified = []
            def verify(binary, receipt, _signtool):
                self.assertEqual(receipt, component)
                self.assertEqual(binary.name, "airk-launcher")
                self.assertNotIn("AIRK.app", binary.parts)
                self.assertEqual(binary.read_bytes(), main.read_bytes())
                verified.append(binary)
            def run(command, **kwargs):
                self.assertEqual(command, [verified[-1], "--version"])
                self.assertEqual(kwargs["timeout"], 30)
                return "airk 1.8.8\n"
            with patch.object(signing, "verify", side_effect=verify), patch.object(signing, "run", side_effect=run):
                observed = signing.verify_copied_launcher(runtime, "1.8.8", component)
            self.assertEqual(observed, {"schema": 1, "binary_sha256": component["binary_sha256"],
                "version": "1.8.8", "strict_signature": True, "standalone_version_probe": True})
            self.assertFalse(verified[0].exists())
            self.assertEqual(main.read_bytes(), b"fictional sealed executable")
            with patch.object(signing, "verify", side_effect=RuntimeError("invalid Info.plist")), patch.object(signing, "run") as execute:
                with self.assertRaisesRegex(RuntimeError, "invalid Info.plist"):
                    signing.verify_copied_launcher(runtime, "1.8.8", component)
                execute.assert_not_called()
            with patch.object(signing, "verify"), patch.object(signing, "run", return_value="airk 1.8.7"):
                with self.assertRaisesRegex(ValueError, "version differs"):
                    signing.verify_copied_launcher(runtime, "1.8.8", component)
            main.write_bytes(b"changed payload")
            with patch.object(signing, "verify") as native, patch.object(signing, "run") as execute:
                with self.assertRaisesRegex(ValueError, "source differs"):
                    signing.verify_copied_launcher(runtime, "1.8.8", component)
                native.assert_not_called(); execute.assert_not_called()

    def test_new_mac_ticket_requires_matching_bundle_receipts_and_native_validation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, output = root / "source", root / "signed"
            runtime = output / "runtime"
            source.mkdir(); runtime.mkdir(parents=True)
            target = "aarch64-apple-darwin"
            names = signing.pair_package().runtime_builder().signing_names("macos")
            original = {"version": "1.6.0", "files": {}}
            signed = {"version": "1.6.0", "files": {}}
            components = {}
            for name in names:
                for parent, body in [(source, b"original component"), (runtime, b"signed component")]:
                    file = parent / name
                    file.parent.mkdir(parents=True, exist_ok=True)
                    file.write_bytes(body)
                original["files"][name] = signing.digest(source / name)
                signed["files"][name] = signing.digest(runtime / name)
                components[name] = {"schema": "airk-native-signing/1", "target": target, "commit": "fixture",
                    "cargo_lock_sha256": "b" * 64, "team_id": "5PX7D99PXA", "source_sha256": original["files"][name], "binary_sha256": signed["files"][name]}
            ticket = runtime / "AIRK.app/Contents/CodeResources"
            ticket.write_bytes(b"fictional ticket; native verifier is mocked in this unit test")
            signed["files"][ticket.relative_to(runtime).as_posix()] = signing.digest(ticket)
            for parent, value in [(source, original), (runtime, signed)]:
                (parent / "airk-install.json").write_text(json.dumps(value), encoding="utf-8")
            receipt = {"schema": "airk-service-pair-signing/1", "target": target, "commit": "fixture", "cargo_lock_sha256": "b" * 64,
                "source_install_sha256": signing.digest(source / "airk-install.json"), "install_manifest_sha256": signing.digest(runtime / "airk-install.json"), "components": components}
            def save(): (output / "signing.json").write_text(json.dumps(receipt), encoding="utf-8")
            save()
            with patch.object(signing, "verify", return_value={}), patch.object(signing, "verify_app_bundle") as bundle, patch.object(signing, "run", return_value="") as native:
                with self.assertRaisesRegex(ValueError, "stapled notarization receipts"):
                    signing.read_verified_pair(output, source, target, "fixture", "b" * 64)
                bundle.assert_not_called(); native.assert_not_called()
                for component in components.values():
                    component["notarization"] = {"id": "matching-native-job", "status": "Accepted", "stapled": True, "scope": "AIRK.app"}
                save()
                signing.read_verified_pair(output, source, target, "fixture", "b" * 64)
                bundle.assert_called_once_with(runtime, "5PX7D99PXA")
                self.assertEqual(native.call_args.args[0], ["/usr/bin/xcrun", "stapler", "validate", runtime / "AIRK.app"])
                components[names[1]]["notarization"]["id"] = "crossed-job"
                save()
                with self.assertRaisesRegex(ValueError, "stapled notarization receipts"):
                    signing.read_verified_pair(output, source, target, "fixture", "b" * 64)
                components[names[1]]["notarization"]["id"] = "matching-native-job"
                original["version"] = signed["version"] = "1.8.8"
                launcher = signing.pair_package().runtime_builder().launcher_name("macos", "1.8.8")
                for parent, body in [(source, b"original standalone image"), (runtime, b"signed standalone image")]:
                    (parent / launcher).write_bytes(body)
                original["files"][launcher] = signing.digest(source / launcher)
                signed["files"][launcher] = signing.digest(runtime / launcher)
                components[launcher] = {**components[names[0]], "source_sha256": original["files"][launcher], "binary_sha256": signed["files"][launcher]}
                for parent, value in [(source, original), (runtime, signed)]:
                    (parent / "airk-install.json").write_text(json.dumps(value), encoding="utf-8")
                receipt["source_install_sha256"] = signing.digest(source / "airk-install.json")
                receipt["install_manifest_sha256"] = signing.digest(runtime / "airk-install.json")
                save()
                copied = {"schema": 1, "binary_sha256": components[launcher]["binary_sha256"],
                    "version": "1.8.8", "strict_signature": True, "standalone_version_probe": True}
                with patch.object(signing, "verify_copied_launcher", return_value=copied) as standalone:
                    with self.assertRaisesRegex(ValueError, "standalone launcher evidence"):
                        signing.read_verified_pair(output, source, target, "fixture", "b" * 64)
                    receipt["copied_launcher"] = copied
                    save()
                    signing.read_verified_pair(output, source, target, "fixture", "b" * 64)
                    self.assertEqual(standalone.call_count, 2)
                    self.assertEqual(standalone.call_args.args[:2], (runtime, "1.8.8"))
                    self.assertEqual(standalone.call_args.args[2], {**components[launcher], "verification": {}})
                    receipt["copied_launcher"] = {**copied, "binary_sha256": "c" * 64}
                    save()
                    with self.assertRaisesRegex(ValueError, "standalone launcher evidence"):
                        signing.read_verified_pair(output, source, target, "fixture", "b" * 64)
    def test_pair_receipt_binds_both_components_and_preserves_unrelated_payload(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source"
            output = root / "signed"
            runtime = output / "runtime"
            source.mkdir()
            runtime.mkdir(parents=True)
            components = {}
            original = {"version": "1.6.0", "files": {"LICENSE": "a" * 64}}
            signed = {"version": "1.6.0", "files": {"LICENSE": "a" * 64}}
            for name in ("airk.exe", "airk-embed.exe"):
                (source / name).write_bytes(("original " + name).encode())
                (runtime / name).write_bytes(("signed " + name).encode())
                original["files"][name] = signing.digest(source / name)
                signed["files"][name] = signing.digest(runtime / name)
                components[name] = {"schema": "airk-native-signing/1", "target": "x86_64-pc-windows-msvc",
                                    "commit": "fixture", "cargo_lock_sha256": "b" * 64,
                                    "source_sha256": original["files"][name], "binary_sha256": signed["files"][name]}
            (source / "airk-install.json").write_text(json.dumps(original), encoding="utf-8")
            (runtime / "airk-install.json").write_text(json.dumps(signed), encoding="utf-8")
            receipt = {"schema": "airk-service-pair-signing/1", "target": "x86_64-pc-windows-msvc", "commit": "fixture",
                       "cargo_lock_sha256": "b" * 64, "source_install_sha256": signing.digest(source / "airk-install.json"),
                       "install_manifest_sha256": signing.digest(runtime / "airk-install.json"), "components": components}
            (output / "signing.json").write_text(json.dumps(receipt), encoding="utf-8")
            with patch.object(signing, "verify", return_value={}) as verify:
                result, _ = signing.read_verified_pair(output, source, receipt["target"], "fixture", "b" * 64)
                self.assertEqual(result, runtime)
                self.assertEqual({call.args[0].name for call in verify.call_args_list}, set(components))
                signed["files"]["LICENSE"] = "c" * 64
                (runtime / "airk-install.json").write_text(json.dumps(signed), encoding="utf-8")
                with self.assertRaisesRegex(ValueError, "unrelated"):
                    signing.read_verified_pair(output, source, receipt["target"], "fixture", "b" * 64)
                del components["airk-embed.exe"]
                (output / "signing.json").write_text(json.dumps(receipt), encoding="utf-8")
                with self.assertRaisesRegex(ValueError, "component inventory"):
                    signing.read_verified_pair(output, source, receipt["target"], "fixture", "b" * 64)

    def test_apple_verification_requires_runtime_timestamp_and_exact_team(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "airk"
            binary.write_bytes(b"fictional signed executable")
            receipt = {"platform": "Darwin", "binary_sha256": signing.digest(binary), "team_id": "ABCDEFGHIJ"}
            details = ("Authority=Developer ID Application: Fixture\nflags=0x10000(runtime)\n"
                       "TeamIdentifier=ABCDEFGHIJ\nTimestamp=fixture\n")
            with patch.object(signing.platform, "system", return_value="Darwin"), \
                    patch.object(signing, "run") as run:
                for removed in ("", "Authority=Developer ID Application:", "runtime", "TeamIdentifier=ABCDEFGHIJ", "Timestamp="):
                    display = subprocess.CompletedProcess([], 0, "", details.replace(removed, "") if removed else details)
                    with self.subTest(removed=removed), patch.object(signing.subprocess, "run", return_value=display):
                        if removed:
                            with self.assertRaises(ValueError):
                                signing.verify(binary, receipt)
                        else:
                            self.assertEqual(signing.verify(binary, receipt)["trust"], "apple_developer_id")
                command = run.call_args.args[0]
                # Without '=', codesign treats the requirement as a filename.
                self.assertTrue(command[command.index("-R") + 1].startswith("=anchor apple generic"))

    def test_explicit_apple_keychain_reaches_signer_and_notary(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "target/aarch64-apple-darwin/release/airk"
            source.parent.mkdir(parents=True)
            source.write_bytes(b"fictional executable")
            (root / "Cargo.lock").write_bytes(b"fictional lock")
            # Exercise canonicalization even when the host temp path has no
            # alias (macOS /var, Windows short names, or a directory symlink).
            (root / "artifacts/staging").mkdir(parents=True)
            output = root / "artifacts/staging/../signed"
            self.assertNotEqual(output, output.resolve())
            keychain = root / "airk keychain"
            def run(command, **kwargs):
                if command[0] == "/usr/bin/codesign":
                    self.assertIn("--force", command)
                    self.assertEqual(command[-1], (output / "airk").resolve())
                    self.assertEqual(source.read_bytes(), b"fictional executable")
                    command[-1].write_bytes(b"fictional Developer ID signed executable")
                if "notarytool" in command:
                    return json.dumps({"id": "fixture-submission", "status": "Accepted"})
                return "fixture-commit"
            arguments = ["sign-native.py", "--target", "aarch64-apple-darwin", "--output", str(output),
                         "--identity", "Developer ID Application: Fixture (ABCDEFGHIJ)",
                         "--team-id", "ABCDEFGHIJ", "--notarize", "--keychain-profile", "airk-notary",
                         "--keychain", str(keychain)]
            with patch.object(signing, "ROOT", root), patch.object(signing, "pair_package", return_value=package_options), patch.object(signing.platform, "system", return_value="Darwin"), \
                    patch.object(sys, "argv", arguments), patch.object(signing, "verify", return_value={}), \
                    patch.object(signing, "run", side_effect=run) as command:
                signing.main()
            calls = [call.args[0] for call in command.call_args_list]
            for tool in ("/usr/bin/codesign", "/usr/bin/xcrun"):
                invocation = next(call for call in calls if call[0] == tool)
                self.assertEqual(invocation[invocation.index("--keychain") + 1], str(keychain))
            receipt = json.loads((output / "signing.json").read_text())
            self.assertEqual(source.read_bytes(), b"fictional executable")
            self.assertEqual(receipt["source_sha256"], signing.digest(source))
            self.assertEqual(receipt["binary_sha256"], signing.digest(output / "airk"))
            self.assertNotEqual(receipt["source_sha256"], receipt["binary_sha256"])
            self.assertEqual(receipt["notarization"]["status"], "Accepted")
            self.assertNotIn("keychain", receipt)

    def test_exact_apple_certificate_sha1_selects_and_verifies_the_leaf(self):
        identity = "4d8496dcab73794378194d040d9177fb28a0f982"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "target/aarch64-apple-darwin/release/airk"
            source.parent.mkdir(parents=True)
            source.write_bytes(b"fictional executable")
            (root / "Cargo.lock").write_bytes(b"fictional lock")
            output = root / "artifacts/signed"
            def command(args, **kwargs):
                if args[0] == "/usr/bin/codesign":
                    self.assertEqual(args[args.index("--sign") + 1], identity)
                    args[-1].write_bytes(b"fictional exact-identity signature")
                return "fixture-commit"
            arguments = ["sign-native.py", "--target", "aarch64-apple-darwin", "--output", str(output),
                         "--identity", identity, "--team-id", "ABCDEFGHIJ"]
            with patch.object(signing, "ROOT", root), patch.object(signing, "pair_package", return_value=package_options), patch.object(signing.platform, "system", return_value="Darwin"), \
                    patch.object(sys, "argv", arguments), patch.object(signing, "verify", return_value={}), \
                    patch.object(signing, "run", side_effect=command):
                signing.main()
            receipt = json.loads((output / "signing.json").read_text())
            self.assertEqual(receipt["identity"], identity)
            details = "Authority=Developer ID Application: Fixture\nflags=0x10000(runtime)\nTeamIdentifier=ABCDEFGHIJ\nTimestamp=fixture\n"
            with patch.object(signing.platform, "system", return_value="Darwin"), patch.object(signing, "run") as run, \
                    patch.object(signing.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "", details)):
                signing.verify(output / "airk", receipt)
                requirement = run.call_args.args[0][run.call_args.args[0].index("-R") + 1]
                self.assertTrue(requirement.startswith("=anchor apple generic"))
                self.assertTrue(requirement.endswith(f'certificate leaf = H"{identity}"'))

    def test_altered_binary_and_source_receipt_fail_before_trust_probe(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root/"source.exe"
            source.write_bytes(b"original fictional executable")
            directory = root/"signed"
            directory.mkdir()
            binary = directory/source.name
            binary.write_bytes(b"fictional signed executable")
            receipt = {"schema":"airk-native-signing/1","target":"x86_64-pc-windows-msvc",
                       "platform":"Windows","commit":"a"*40,"cargo_lock_sha256":"b"*64,
                       "source_sha256":signing.digest(source),"binary_sha256":signing.digest(binary),"identity":"c"*40}
            (directory/"signing.json").write_text(json.dumps(receipt),encoding="utf-8")
            with patch.object(signing.platform,"system",return_value="Windows"), patch.object(signing,"run") as run:
                binary.write_bytes(b"tampered")
                with self.assertRaises(ValueError):
                    signing.read_verified(directory,source,receipt["target"],receipt["commit"],receipt["cargo_lock_sha256"])
                run.assert_not_called()
                source.write_bytes(b"other build")
                with self.assertRaises(ValueError):
                    signing.read_verified(directory,source,receipt["target"],receipt["commit"],receipt["cargo_lock_sha256"])
                run.assert_not_called()

    def test_certificate_trust_failure_cannot_produce_verified_result(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary)/"airk.exe"
            binary.write_bytes(b"fictional executable")
            receipt = {"platform":"Windows","binary_sha256":signing.digest(binary),"identity":"c"*40}
            with patch.object(signing.platform,"system",return_value="Windows"), patch.object(signing,"run",side_effect=RuntimeError("untrusted chain")):
                with self.assertRaises(RuntimeError):
                    signing.verify(binary,receipt)


ROOT = Path(__file__).resolve().parents[2]
# Reviewed literal (split plan C10): hosted receipts compare this mapping exactly.
REVIEWED_AZURE_SIGNING = {
    "endpoint": "https://wus2.codesigning.azure.net",
    "account": "ioka-llc-signing",
    "profile": "WritingForgePro",
    "publisher": "CN=Ioka LLC, O=Ioka LLC, L=Mapleton, S=Utah, C=US",
    "identity_eku": "1.3.6.1.4.1.311.97.88309284.513035131.587831003.613935669",
}


class SplitPairSigning(unittest.TestCase):
    """Split versions (1.8.24+) sign the airk + airkctl pair; names are the reviewed §9.5 images."""
    PAIRS = {"x86_64-pc-windows-msvc": ("airk.exe", "airkctl.exe"),
             "aarch64-apple-darwin": ("AIRK.app/Contents/MacOS/airk", "AIRK.app/Contents/MacOS/airkctl")}

    def pair(self, root, target, version="1.8.24"):
        source, output = root / "source", root / "signed"
        runtime = output / "runtime"
        source.mkdir(); runtime.mkdir(parents=True)
        original = {"schema": 1, "version": version, "files": {"LICENSE": "a" * 64}}
        signed = {"schema": 1, "version": version, "files": {"LICENSE": "a" * 64}}
        components = {}
        for name in self.PAIRS[target]:
            for parent, body in [(source, b"original " + name.encode()), (runtime, b"signed " + name.encode())]:
                (parent / name).parent.mkdir(parents=True, exist_ok=True)
                (parent / name).write_bytes(body)
            original["files"][name] = signing.digest(source / name)
            signed["files"][name] = signing.digest(runtime / name)
            components[name] = {"schema": "airk-native-signing/1", "target": target, "commit": "fixture",
                                "cargo_lock_sha256": "b" * 64, "team_id": "5PX7D99PXA",
                                "source_sha256": original["files"][name], "binary_sha256": signed["files"][name],
                                "notarization": {"id": "one-app-job", "status": "Accepted", "stapled": True,
                                                 "scope": "AIRK.app"}}
        for parent, value in [(source, original), (runtime, signed)]:
            (parent / "airk-install.json").write_text(json.dumps(value), encoding="utf-8")
        receipt = {"schema": "airk-service-pair-signing/1", "target": target, "commit": "fixture",
                   "cargo_lock_sha256": "b" * 64, "source_install_sha256": signing.digest(source / "airk-install.json"),
                   "install_manifest_sha256": signing.digest(runtime / "airk-install.json"), "components": components}
        (output / "signing.json").write_text(json.dumps(receipt), encoding="utf-8")
        return source, output, runtime, receipt

    def test_split_signing_names_are_both_images(self):
        builder = signing.pair_package().runtime_builder()
        self.assertEqual(builder.signing_names("windows", "1.8.24"), ("airk.exe", "airkctl.exe"))
        self.assertEqual(builder.signing_names("macos", "1.8.24"),
                         ("AIRK.app/Contents/MacOS/airk", "AIRK.app/Contents/MacOS/airkctl"))

    def test_read_verified_pair_accepts_split_windows_and_macos_pairs_and_refuses_one_image(self):
        for target in self.PAIRS:
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temporary:
                source, output, runtime, receipt = self.pair(Path(temporary), target)
                with patch.object(signing, "verify", return_value={}) as verify, \
                        patch.object(signing, "verify_app_bundle") as bundle, \
                        patch.object(signing, "run", return_value=""):
                    result, _ = signing.read_verified_pair(output, source, target, "fixture", "b" * 64)
                    self.assertEqual(result, runtime)
                    self.assertEqual({call.args[0].relative_to(runtime).as_posix() for call in verify.call_args_list},
                                     set(self.PAIRS[target]))
                    self.assertEqual(bundle.call_count, 1 if "apple" in target else 0)
                    del receipt["components"][self.PAIRS[target][1]]
                    (output / "signing.json").write_text(json.dumps(receipt), encoding="utf-8")
                    with self.assertRaisesRegex(ValueError, "component inventory"):
                        signing.read_verified_pair(output, source, target, "fixture", "b" * 64)

    def test_finish_pair_records_both_signed_digests_in_schema_3_pair_metadata(self):
        with tempfile.TemporaryDirectory() as temporary:
            target = "x86_64-pc-windows-msvc"
            source, output, runtime, receipt = self.pair(Path(temporary), target)
            pair = {"schema": 3, "version": "1.8.24", "os": "windows", "arch": "x86_64",
                    "airk_sha256": signing.digest(source / "airk.exe"),
                    "airkctl_sha256": signing.digest(source / "airkctl.exe"),
                    "i1_protocol": 1, "i2_contract": 1, "admin_inventory_sha256": "3" * 64}
            # prepare_pair stages an exact copy; only the two images have been re-signed since.
            (runtime / "airk-install.json").write_bytes((source / "airk-install.json").read_bytes())
            for parent in (source, runtime):
                (parent / "airk-runtime.json").write_text(json.dumps(pair), encoding="utf-8")
                (parent / "airk-build.json").write_text(json.dumps({"schema": 2, "source": "c" * 64, "runtime": pair}),
                                                        encoding="utf-8")
            package = signing.pair_package()
            files = package.inventory(source)
            (output / "pair-signing.pending.json").write_text(json.dumps(
                {"schema": 1, "source": str(source), "target": target,
                 "source_install_sha256": files["airk-install.json"]}), encoding="utf-8")
            (output / "signing.json").unlink()
            components = {name: dict(value, notarization=None) for name, value in receipt["components"].items()}
            with patch.object(signing, "pair_package", return_value=package), \
                    patch.object(package, "validate_runtime", side_effect=lambda root, *_: (package.inventory(root), self.PAIRS[target])), \
                    patch.object(signing, "verify", return_value={}), patch("builtins.print"):
                signing.finish_pair(source, output, target, components, "fixture", "b" * 64)
            recorded = json.loads((runtime / "airk-runtime.json").read_text(encoding="utf-8"))
            self.assertEqual(recorded["airk_sha256"], signing.digest(runtime / "airk.exe"))
            self.assertEqual(recorded["airkctl_sha256"], signing.digest(runtime / "airkctl.exe"))
            self.assertNotIn("embed_sha256", recorded)
            self.assertEqual(json.loads((runtime / "airk-build.json").read_text(encoding="utf-8"))["runtime"], recorded)
            written = json.loads((output / "signing.json").read_text(encoding="utf-8"))
            self.assertEqual(written["schema"], "airk-service-pair-signing/1")
            self.assertEqual(set(written["components"]), {"airk.exe", "airkctl.exe"})
            self.assertEqual(written["install_manifest_sha256"], signing.digest(runtime / "airk-install.json"))


class SigningIdentityFile(unittest.TestCase):
    def test_hosted_policy_is_unchanged_in_value_and_order(self):
        self.assertEqual(signing.AZURE_SIGNING, REVIEWED_AZURE_SIGNING)
        self.assertEqual(json.dumps(signing.AZURE_SIGNING), json.dumps(REVIEWED_AZURE_SIGNING))

    def test_loader_refuses_unknown_missing_or_malformed_fields(self):
        original = json.loads(signing.IDENTITY_FILE.read_text(encoding="utf-8"))
        def mutated(change):
            document = json.loads(json.dumps(original))
            change(document)
            with tempfile.TemporaryDirectory() as temporary:
                path = Path(temporary) / "identity.json"
                path.write_text(json.dumps(document), encoding="utf-8")
                return signing.load_identity(path)
        cases = [
            lambda d: d.update(schema="airk-signing-identity/2"),
            lambda d: d["windows"].update(extra="value"),
            lambda d: d["windows"]["artifact_signing"].pop("account"),
            lambda d: d["macos"]["local"].update(password="never"),
            lambda d: d["windows"]["local_client"].update(dlib_sha256="ABC"),
            lambda d: d["macos"].update(team_id="short"),
        ]
        for change in cases:
            with self.assertRaises((ValueError, KeyError)):
                mutated(change)
        self.assertEqual(mutated(lambda d: None)["macos"]["team_id"], "5PX7D99PXA")

    def test_hosted_workflow_inputs_match_the_identity_file(self):
        azure = signing.SIGNING_IDENTITY["windows"]["artifact_signing"]
        workflow = (ROOT / ".github/workflows/github-release.yml").read_text(encoding="utf-8")
        blocks = workflow.split("uses: azure/artifact-signing-action@")[1:]
        self.assertEqual(len(blocks), 3)
        for block in blocks:
            inputs = dict(line.strip().split(": ", 1) for line in block.split("\n")[2:16] if ": " in line)
            self.assertEqual(inputs["endpoint"], azure["endpoint"])
            self.assertEqual(inputs["signing-account-name"], azure["account"])
            self.assertEqual(inputs["certificate-profile-name"], azure["certificate_profile"])
            self.assertEqual(inputs["file-digest"], azure["file_digest"])
            self.assertEqual(inputs["timestamp-rfc3161"], azure["timestamp_url"])
            self.assertEqual(inputs["timestamp-digest"], azure["timestamp_digest"])

    def test_macos_constants_match_the_identity_file(self):
        macos = signing.SIGNING_IDENTITY["macos"]
        sign = (ROOT / "scripts/ci-macos-sign.py").read_text(encoding="utf-8")
        self.assertIn(f'IDENTITY = "{macos["identity"]}"', sign)
        self.assertIn(f'TEAM = "{macos["team_id"]}"', sign)
        self.assertIn(f'CERTIFICATE_SHA256 = "{macos["certificate_sha256"]}"', sign)
        self.assertIn(f'TEAM = "{macos["team_id"]}"', (ROOT / "scripts/check-macos-direct-runtime.py").read_text(encoding="utf-8"))
        specification = importlib.util.spec_from_file_location("test_macos_packaging_policy", ROOT / "scripts/package-standalone-macos.py")
        owner = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(owner)
        self.assertEqual(owner.TEAM, macos["team_id"])
        self.assertEqual(owner.CERTIFICATE_SHA256, macos["certificate_sha256"])


class LocalPairSigning(unittest.TestCase):
    """Actual receipt and pair byte checks; OS verification alone is injected."""
    def pair(self, root):
        target = "x86_64-pc-windows-msvc"
        source, output, runtime, receipt = SplitPairSigning().pair(root, target)
        fixture = json.loads((ROOT / "fixtures/split/receipts/native-signing-local.json").read_bytes())["base"]
        receipt.update(schema="airk-service-pair-signing-local/1", provenance="local", commit="a" * 40)
        for name, previous in list(receipt["components"].items()):
            component = json.loads(json.dumps(fixture))
            component.update(source_sha256=previous["source_sha256"], binary_sha256=previous["binary_sha256"],
                             commit="a" * 40, cargo_lock_sha256="b" * 64)
            receipt["components"][name] = component
        (output / "signing.json").write_text(json.dumps(receipt), encoding="utf-8")
        return source, output, runtime, receipt

    def test_local_pair_is_explicit_and_reverifies_both_os_signatures(self):
        with tempfile.TemporaryDirectory() as directory:
            source, output, runtime, receipt = self.pair(Path(directory))
            with patch.object(signing, "verify", side_effect=lambda _binary, component, _tool: component["verification"]) as verify:
                with self.assertRaisesRegex(ValueError, "Pair signing receipt differs"):
                    signing.read_verified_pair(output, source, receipt["target"], "a" * 40, "b" * 64)
                verify.assert_not_called()
                actual, observed = signing.read_verified_pair(output, source, receipt["target"], "a" * 40, "b" * 64, allow_local=True)
                self.assertEqual(actual, runtime)
                self.assertEqual(observed["schema"], "airk-service-pair-signing-local/1")
                self.assertEqual(observed["provenance"], "local")
                self.assertEqual({call.args[0].name for call in verify.call_args_list}, {"airk.exe", "airkctl.exe"})
                self.assertEqual(verify.call_count, 2)

    def test_local_pair_rejects_mixed_origins_wrong_client_policy_and_lost_timestamp(self):
        mutations = [
            lambda r: r["components"]["airk.exe"].update(schema="airk-native-signing/1"),
            lambda r: r.update(provenance="hosted"),
            lambda r: r["components"]["airk.exe"]["local_signing"]["client"].update(dlib_sha256="1" * 64),
            lambda r: r["components"]["airk.exe"]["azure"].update(publisher="unreviewed publisher"),
            lambda r: r["components"]["airk.exe"]["verification"].update(timestamp_present=False),
            lambda r: r["components"]["airk.exe"].update(commit="c" * 40),
            lambda r: r["components"]["airk.exe"].update(source_sha256="d" * 64),
        ]
        for change in mutations:
            with self.subTest(change=change), tempfile.TemporaryDirectory() as directory:
                source, output, _runtime, receipt = self.pair(Path(directory))
                change(receipt)
                (output / "signing.json").write_text(json.dumps(receipt), encoding="utf-8")
                with patch.object(signing, "verify") as verify:
                    with self.assertRaises(ValueError):
                        signing.read_verified_pair(output, source, receipt["target"], "a" * 40, "b" * 64, allow_local=True)
                    verify.assert_not_called()

    def test_local_pair_still_rejects_changed_images_and_unrelated_payload(self):
        for name in ("airk.exe", "airkctl.exe", "LICENSE"):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                source, output, runtime, receipt = self.pair(Path(directory))
                (runtime / name).write_bytes(b"changed bytes")
                if name == "LICENSE":
                    manifest = json.loads((runtime / "airk-install.json").read_bytes())
                    manifest["files"][name] = signing.digest(runtime / name)
                    (runtime / "airk-install.json").write_text(json.dumps(manifest), encoding="utf-8")
                    receipt["install_manifest_sha256"] = signing.digest(runtime / "airk-install.json")
                    (output / "signing.json").write_text(json.dumps(receipt), encoding="utf-8")
                def verify(binary, component, _tool):
                    if signing.digest(binary) != component["binary_sha256"]:
                        raise ValueError("changed signature bytes")
                    return component["verification"]
                with patch.object(signing, "verify", side_effect=verify):
                    with self.assertRaises(ValueError):
                        signing.read_verified_pair(output, source, receipt["target"], "a" * 40, "b" * 64, allow_local=True)

    def test_local_cli_does_not_take_hosted_identity_or_apple_options(self):
        base = ["sign-native.py", "--target", "x86_64-pc-windows-msvc", "--runtime-dir", "fixture", "--output", "fixture-out", "--local-artifact-signing"]
        with patch.object(signing.platform, "system", return_value="Windows"), patch.object(signing, "sign_local_pair") as local:
            with patch.object(sys, "argv", base):
                signing.main()
            self.assertEqual(local.call_count, 1)
            for extra in (["--identity", "a" * 40], ["--timestamp-url", "https://unreviewed.invalid"], ["--notarize"]):
                local.reset_mock()
                with patch.object(sys, "argv", base + extra), patch("sys.stderr"), self.assertRaises(SystemExit):
                    signing.main()
                local.assert_not_called()

    def test_local_finish_seals_metadata_and_never_relabels_provenance(self):
        with tempfile.TemporaryDirectory() as directory:
            source, output, runtime, receipt = self.pair(Path(directory))
            target = receipt["target"]
            pair = {"schema": 3, "version": "1.8.24", "os": "windows", "arch": "x86_64",
                    "airk_sha256": signing.digest(source / "airk.exe"),
                    "airkctl_sha256": signing.digest(source / "airkctl.exe"),
                    "i1_protocol": 1, "i2_contract": 1, "admin_inventory_sha256": "3" * 64}
            (runtime / "airk-install.json").write_bytes((source / "airk-install.json").read_bytes())
            for parent in (source, runtime):
                (parent / "airk-runtime.json").write_text(json.dumps(pair), encoding="utf-8")
                (parent / "airk-build.json").write_text(json.dumps({"schema": 2, "source": "c" * 64, "runtime": pair}), encoding="utf-8")
            package = signing.pair_package()
            before = package.inventory(source)
            (output / "pair-signing.pending.json").write_text(json.dumps(
                {"schema": 1, "source": str(source), "target": target,
                 "source_install_sha256": before["airk-install.json"]}), encoding="utf-8")
            (output / "signing.json").unlink()
            with patch.object(signing, "pair_package", return_value=package), \
                    patch.object(package, "validate_runtime", side_effect=lambda root, *_: (package.inventory(root), ("airk.exe", "airkctl.exe"))), \
                    patch.object(signing, "verify", side_effect=lambda _path, component, _tool: component["verification"]) as verify, patch("builtins.print"):
                signing.finish_pair(source, output, target, receipt["components"], "a" * 40, "b" * 64, local=True)
            self.assertEqual(verify.call_count, 2)
            written = json.loads((output / "signing.json").read_bytes())
            signing.signing_receipts().validate_service_pair_signing_local(written)
            self.assertEqual((written["schema"], written["provenance"]), ("airk-service-pair-signing-local/1", "local"))
            self.assertEqual(written["source_install_sha256"], before["airk-install.json"])
            self.assertEqual(written["install_manifest_sha256"], signing.digest(runtime / "airk-install.json"))
            actual = json.loads((runtime / "airk-runtime.json").read_bytes())
            for name, field in (("airk.exe", "airk_sha256"), ("airkctl.exe", "airkctl_sha256")):
                self.assertEqual(actual[field], signing.digest(runtime / name))
            self.assertEqual(json.loads((runtime / "airk-build.json").read_bytes())["runtime"], actual)
            self.assertEqual(package.inventory(source), before)

    def test_local_signing_uses_existing_adapter_once_and_seals_only_after_both_verified(self):
        for failed in (False, True):
            with self.subTest(failed=failed), tempfile.TemporaryDirectory() as directory:
                source, output, runtime, receipt = self.pair(Path(directory))
                (output / "signing.json").unlink()
                def sign(original, destination, target, commit, lock_digest, **options):
                    self.assertEqual(options, {"dlib": "checked-client", "signtool": Path("signtool-fixture")})
                    if failed and original.name == "airkctl.exe":
                        raise RuntimeError("second signature refused")
                    destination.parent.mkdir()
                    destination.write_bytes(b"new signed " + original.read_bytes())
                    component = dict(receipt["components"][original.name],
                                     binary_sha256=signing.digest(destination), cargo_lock_sha256=lock_digest)
                    return component
                adapter = SimpleNamespace(provision=Mock(return_value="checked-client"), sign=Mock(side_effect=sign),
                                          signtool_path=Mock(return_value=Path("signtool-fixture")))
                specification = SimpleNamespace(loader=SimpleNamespace(exec_module=Mock()))
                package = SimpleNamespace(runtime_builder=lambda: SimpleNamespace(signing_names=lambda *_: ("airk.exe", "airkctl.exe")))
                args = SimpleNamespace(output=output, runtime_dir=source, target=receipt["target"], signtool="signtool")
                # Load receipt validators before injecting only the adapter import.
                validators = signing.signing_receipts()
                with patch.object(signing.platform, "system", return_value="Windows"), \
                        patch("importlib.util.spec_from_file_location", return_value=specification), \
                        patch("importlib.util.module_from_spec", return_value=adapter), \
                        patch.object(signing, "signing_receipts", return_value=validators), \
                        patch.object(signing, "prepare_pair", return_value=(source, runtime, (), {})), \
                        patch.object(signing, "pair_package", return_value=package), \
                        patch.object(signing, "run", return_value="a" * 40) as run, \
                        patch.object(signing, "verify", side_effect=lambda _path, component, _tool: component["verification"]) as verify, \
                        patch.object(signing, "finish_pair") as finish:
                    if failed:
                        with self.assertRaisesRegex(RuntimeError, "second signature refused"):
                            signing.sign_local_pair(args)
                        finish.assert_not_called()
                        self.assertFalse((output / "signing.json").exists())
                    else:
                        signing.sign_local_pair(args)
                        finish.assert_called_once()
                        self.assertEqual(finish.call_args.kwargs, {"local": True})
                        self.assertEqual(set(finish.call_args.args[3]), {"airk.exe", "airkctl.exe"})
                        self.assertEqual(verify.call_count, 2)
                        for name in ("airk.exe", "airkctl.exe"):
                            self.assertEqual((runtime / name).read_bytes(), (output / (name + ".signature") / name).read_bytes())
                    adapter.provision.assert_called_once_with()
                    run.assert_called_once_with(["git", "rev-parse", "HEAD"])


if __name__ == "__main__":
    unittest.main()
