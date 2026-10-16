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
