# SPDX-License-Identifier: Apache-2.0
import importlib.util
import json
from pathlib import Path
import sys
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("native_signing",Path(__file__).parents[1]/"sign-native.py")
signing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(signing)


class SigningContracts(unittest.TestCase):
    def test_new_mac_ticket_requires_matching_bundle_receipts_and_native_validation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, output = root / "source", root / "signed"
            runtime = output / "runtime"
            source.mkdir(); runtime.mkdir(parents=True)
            target = "aarch64-apple-darwin"
            names = signing.pair_package().runtime_builder().runtime_names("macos")
            original = {"files": {}}
            signed = {"files": {}}
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
    def test_pair_receipt_binds_both_components_and_preserves_unrelated_payload(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source"
            output = root / "signed"
            runtime = output / "runtime"
            source.mkdir()
            runtime.mkdir(parents=True)
            components = {}
            original = {"files": {"LICENSE": "a" * 64}}
            signed = {"files": {"LICENSE": "a" * 64}}
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
            with patch.object(signing, "ROOT", root), patch.object(signing.platform, "system", return_value="Darwin"), \
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


if __name__ == "__main__":
    unittest.main()
