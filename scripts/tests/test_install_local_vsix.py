# SPDX-License-Identifier: Apache-2.0
"""Exercise local VSIX verification without launching VS Code or installing AIRK."""
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import zipfile


ROOT = Path(__file__).resolve().parents[2]
PWSH = shutil.which("pwsh")


@unittest.skipUnless(PWSH, "PowerShell 7 is unavailable")
class InstallLocalVsixTests(unittest.TestCase):
    def verify(self, changed=None):
        with tempfile.TemporaryDirectory(prefix="airk-vsix-verification-") as directory:
            root = Path(directory)
            extensions = root / "extensions"
            installed = extensions / "fixture.airk-1.0.0" / "out"
            installed.mkdir(parents=True)
            archive_path = root / "fixture.vsix"
            assets = {name: f"fixture {name}" for name in (
                "serviceExtension.js", "explorerWebview.js", "explorerWebview.css", "workbenchView.js"
            )}
            with zipfile.ZipFile(archive_path, "w") as archive:
                archive.writestr("extension/package.json", json.dumps({
                    "publisher": "fixture", "name": "airk", "version": "1.0.0"
                }))
                for name, contents in assets.items():
                    archive.writestr("extension/out/" + name, contents)
                    (installed / name).write_text(contents if name != changed else "stale UI")
            # The candidate files already exist; the fake CLI performs no install.
            cli = root / "code.ps1"
            cli.write_text("$global:LASTEXITCODE = 0\n")
            return subprocess.run([
                PWSH, "-NoProfile", "-File", str(ROOT / "scripts/install-local-vsix.ps1"),
                "-Vsix", str(archive_path), "-Code", str(cli), "-ExtensionsDir", str(extensions)
            ], capture_output=True, text=True)

    def test_matching_host_and_ui_pass(self):
        result = self.verify()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_ui_mismatch_fails_even_when_host_matches(self):
        for name in ("explorerWebview.js", "explorerWebview.css", "workbenchView.js"):
            with self.subTest(asset=name):
                result = self.verify(name)
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn(name, result.stdout + result.stderr)
                self.assertIn("does not match the VSIX", result.stdout + result.stderr)
