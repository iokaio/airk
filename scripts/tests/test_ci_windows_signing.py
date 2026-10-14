# SPDX-License-Identifier: Apache-2.0
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("ci_windows", Path(__file__).parents[1] / "ci-windows-sign.py")
signing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(signing)


class WindowsSigningContracts(unittest.TestCase):
    def test_host_and_workflow_guards(self):
        environment = {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted",
                       "GITHUB_REF": "refs/heads/main", "GITHUB_EVENT_NAME": "workflow_dispatch",
                       "GITHUB_WORKFLOW_REF": "iokaio/airk/.github/workflows/github-release.yml@refs/heads/main"}
        with patch.dict(os.environ, environment, clear=True), patch.object(signing.sys, "platform", "win32"):
            signing.guard()
            for key, value in (("GITHUB_REF", "refs/pull/1/merge"), ("GITHUB_EVENT_NAME", "pull_request"),
                               ("RUNNER_ENVIRONMENT", "self-hosted"), ("GITHUB_WORKFLOW_REF", "untrusted")):
                with patch.dict(os.environ, {key: value}), self.assertRaises(ValueError):
                    signing.guard()

    def test_trust_failure_never_writes_success_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source.exe"
            source.write_bytes(b"original fixture")
            output = root / "signed"
            output.mkdir()
            (output / "airk.exe").write_bytes(b"signed fixture")
            with patch.object(signing, "guard"), patch.object(signing, "paths", return_value=(source, output)), \
                    patch.object(signing.signer, "run", side_effect=RuntimeError("trust failure")):
                with self.assertRaisesRegex(RuntimeError, "trust failure"):
                    signing.record()
                self.assertFalse((output / "signing.json").exists())
                self.assertEqual(source.read_bytes(), b"original fixture")

    @unittest.skipUnless(shutil.which("pwsh"), "PowerShell 7 required")
    def test_real_certificate_eku_parser_and_policy_refusals(self):
        # Only the OS signature lookup is a fixture. Exercise the actual verifier
        # against a generated in-memory certificate, without installing any key.
        script = r'''
param([string]$Case)
$ErrorActionPreference = 'Stop'
$airkRsa = [System.Security.Cryptography.RSA]::Create(2048)
$airkRequest = [System.Security.Cryptography.X509Certificates.CertificateRequest]::new(
    'CN=Fictional Publisher', $airkRsa, [System.Security.Cryptography.HashAlgorithmName]::SHA256,
    [System.Security.Cryptography.RSASignaturePadding]::Pkcs1)
$airkOids = [System.Security.Cryptography.OidCollection]::new()
$null = $airkOids.Add([System.Security.Cryptography.Oid]::new('1.2.3.4.5'))
$airkRequest.CertificateExtensions.Add(
    [System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension]::new($airkOids, $false))
$airkCertificate = $airkRequest.CreateSelfSigned([DateTimeOffset]::UtcNow.AddMinutes(-1), [DateTimeOffset]::UtcNow.AddHours(1))
function Get-AuthenticodeSignature {
    param($LiteralPath)
    [pscustomobject]@{
        Status = $(if ($Case -eq 'trust') { 'NotTrusted' } else { 'Valid' })
        SignerCertificate = $airkCertificate
        TimeStamperCertificate = $(if ($Case -eq 'timestamp') { $null } else { $airkCertificate })
    }
}
$airkArguments = @{Binary='fictional.exe';Publisher='CN=Fictional Publisher';IdentityEku='1.2.3.4.5'}
if ($Case -eq 'publisher') { $airkArguments.Publisher = 'CN=Different Publisher' }
if ($Case -eq 'eku') { $airkArguments.IdentityEku = '1.2.3.4.6' }
if ($Case -eq 'thumbprint') { $airkArguments.Thumbprint = '0' * 40 }
& $env:AIRK_VERIFY_SCRIPT @airkArguments
$airkCertificate.Dispose()
$airkRsa.Dispose()
'''
        with tempfile.TemporaryDirectory() as temporary:
            fixture = Path(temporary) / "verify-fixture.ps1"
            fixture.write_text(script, encoding="utf-8")
            environment = dict(os.environ, AIRK_VERIFY_SCRIPT=str(signing.ROOT / "scripts/verify-authenticode.ps1"))
            for case in ("valid", "publisher", "eku", "thumbprint", "timestamp", "trust"):
                with self.subTest(case=case):
                    result = subprocess.run(["pwsh", "-NoProfile", "-NonInteractive", "-File", str(fixture), case],
                                            env=environment, capture_output=True, text=True, timeout=30)
                    if case == "valid":
                        self.assertEqual(result.returncode, 0, result.stderr)
                        self.assertTrue(json.loads(result.stdout)["timestamp_present"])
                    else:
                        self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
