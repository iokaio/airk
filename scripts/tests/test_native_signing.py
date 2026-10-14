# SPDX-License-Identifier: Apache-2.0
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("native_signing",Path(__file__).parents[1]/"sign-native.py")
signing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(signing)


class SigningContracts(unittest.TestCase):
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
