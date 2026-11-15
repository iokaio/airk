# SPDX-License-Identifier: Apache-2.0
"""The U70 local signing adapter: pinned client, CI settings, fail-closed (mocked)."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import zipfile

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("airk_local_signing", ROOT / "scripts" / "local_signing.py")
LOCAL = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(LOCAL)


def package(entries):
    data = io.BytesIO()
    with zipfile.ZipFile(data, "w") as archive:
        for name, body in entries.items():
            archive.writestr(name, body)
    return data.getvalue()


class Response(io.BytesIO):
    def __enter__(self):
        return self

    def __exit__(self, *_):
        return False


class LocalSigning(unittest.TestCase):
    def test_metadata_is_exactly_the_checked_in_ci_account(self):
        meta = LOCAL.metadata()
        self.assertEqual(meta["Endpoint"], LOCAL.SIGNING["endpoint"])
        self.assertEqual(meta["CodeSigningAccountName"], LOCAL.SIGNING["account"])
        self.assertEqual(meta["CertificateProfileName"], LOCAL.SIGNING["certificate_profile"])
        self.assertEqual(meta["ExcludeCredentials"], LOCAL.SIGNING["excluded_credentials"])
        self.assertNotIn("AzureCliCredential", meta["ExcludeCredentials"])

    def test_the_client_lives_outside_the_repository_and_airk_roots(self):
        # The qualification runner may place TEMP inside the real checkout.
        # Model a sibling checkout/cache layout so this test reaches its own boundary.
        with tempfile.TemporaryDirectory() as home, \
                mock.patch.dict("os.environ", {"LOCALAPPDATA": home}), \
                mock.patch.object(LOCAL, "ROOT", Path(home) / "repository"):
            root = LOCAL.client_root()
            self.assertEqual(root.parent.name, "airk-artifact-signing-client")
            self.assertNotIn("AIRK", root.parts)
            self.assertNotIn(LOCAL.ROOT, root.parents)

    def test_a_package_with_the_wrong_digest_is_refused_before_extraction(self):
        # The qualification runner may place TEMP inside the real checkout.
        # Model a sibling checkout/cache layout so this test reaches its own boundary.
        with tempfile.TemporaryDirectory() as home, \
                mock.patch.dict("os.environ", {"LOCALAPPDATA": home}), \
                mock.patch.object(LOCAL, "ROOT", Path(home) / "repository"):
            fake = package({"bin/x64/Azure.CodeSigning.Dlib.dll": b"not the pinned client"})
            with self.assertRaisesRegex(RuntimeError, "digest differs"):
                LOCAL.provision(download=lambda url: Response(fake), run=lambda *a, **k: "{}")
            self.assertFalse(LOCAL.client_root().exists())

    def test_a_client_inside_the_repository_is_refused_before_download_or_extraction(self):
        with tempfile.TemporaryDirectory() as home, \
                mock.patch.dict("os.environ", {"LOCALAPPDATA": str(Path(home) / "local")}), \
                mock.patch.object(LOCAL, "ROOT", Path(home)), \
                mock.patch.object(LOCAL, "extract") as extract:
            download = mock.Mock(side_effect=AssertionError("repository cache must refuse before download"))
            with self.assertRaisesRegex(RuntimeError, "signing client must live outside the repository"):
                LOCAL.provision(download=download, run=lambda *a, **k: self.fail("must not inspect a client"))
            download.assert_not_called()
            extract.assert_not_called()
            self.assertEqual(list(Path(home).iterdir()), [])

    def test_package_entries_cannot_escape_the_client_folder(self):
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder) / "client"
            with self.assertRaisesRegex(RuntimeError, "escapes"):
                LOCAL.extract(package({"../outside.dll": b"x"}), target)
            self.assertFalse(target.exists())

    def test_a_dlib_that_is_not_microsoft_signed_is_refused(self):
        with tempfile.TemporaryDirectory() as folder:
            dlib = Path(folder) / "dlib.dll"
            dlib.write_bytes(b"pinned")
            with mock.patch.dict(LOCAL.CLIENT, {"dlib_sha256": hashlib.sha256(b"pinned").hexdigest()}):
                unsigned = json.dumps({"status": "NotSigned", "subject": "", "thumbprint": ""})
                with self.assertRaisesRegex(RuntimeError, "not validly signed"):
                    LOCAL.check_client(dlib, run=lambda *a, **k: unsigned)
                signed = json.dumps({"status": "Valid", "subject": LOCAL.CLIENT["dlib_publisher"], "thumbprint": "A" * 40})
                self.assertEqual(LOCAL.check_client(dlib, run=lambda *a, **k: signed), dlib)
            dlib.write_bytes(b"changed")
            with self.assertRaisesRegex(RuntimeError, "pinned digest"):
                LOCAL.check_client(dlib, run=lambda *a, **k: "{}")

    def test_a_failed_signature_leaves_no_output(self):
        with tempfile.TemporaryDirectory() as folder:
            source, output = Path(folder) / "airk.exe", Path(folder) / "out" / "airk.exe"
            source.write_bytes(b"binary")
            tool = Path(folder) / "signtool.exe"
            tool.write_bytes(b"tool")

            def run(args, timeout=180):
                raise RuntimeError("signtool failed (1): no access")

            with mock.patch.object(LOCAL.platform, "system", return_value="Windows"):
                with self.assertRaisesRegex(RuntimeError, "no access"):
                    LOCAL.sign(source, output, "windows-x64", "a" * 40, "b" * 64,
                               dlib=Path(folder) / "dlib.dll", signtool=tool, run=run)
            self.assertFalse(output.exists())

    def test_signing_never_overwrites(self):
        with tempfile.TemporaryDirectory() as folder:
            source, output = Path(folder) / "airk.exe", Path(folder) / "signed.exe"
            source.write_bytes(b"binary")
            output.write_bytes(b"older")
            with mock.patch.object(LOCAL.platform, "system", return_value="Windows"):
                with self.assertRaisesRegex(RuntimeError, "never overwrites"):
                    LOCAL.sign(source, output, "windows-x64", "a" * 40, "b" * 64,
                               dlib=Path(folder), signtool=Path(folder), run=lambda *a, **k: "")
            self.assertEqual(output.read_bytes(), b"older")


if __name__ == "__main__":
    unittest.main()
