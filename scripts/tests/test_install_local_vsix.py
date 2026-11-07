# SPDX-License-Identifier: Apache-2.0
"""Exercise local VSIX verification without launching VS Code or installing AIRK."""
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile


ROOT = Path(__file__).resolve().parents[2]
PWSH = shutil.which("pwsh")


class InstallLocalVsixShellGuidanceTests(unittest.TestCase):
    def test_shell_verification_reports_guidance_for_the_actual_archive_inventory(self):
        # Execute the shell's Python verifier directly against isolated files;
        # this exercises its branching without a real CLI or a user profile.
        source = (ROOT / "scripts/install-local-vsix.sh").read_text(encoding="utf-8")
        verifier = source.split('"$python" -c \'import hashlib,sys,zipfile\n', 1)[1].split(
            '\' "$vsix" "$extensions/$id_version"', 1
        )[0]
        verifier = "import hashlib,sys,zipfile\n" + verifier
        for update_loader in (False, True):
            with self.subTest(update_loader=update_loader), tempfile.TemporaryDirectory(prefix="airk-shell-vsix-") as directory:
                root = Path(directory)
                output = root / "installed" / "out"
                output.mkdir(parents=True)
                archive_path = root / "fixture.vsix"
                with zipfile.ZipFile(archive_path, "w") as archive:
                    for name in ("serviceExtension.js", "explorerWebview.js", "explorerWebview.css"):
                        archive.writestr("extension/out/" + name, name)
                        (output / name).write_text(name)
                    if update_loader:
                        archive.writestr("extension/airk-update.json", "{}")
                result = subprocess.run([sys.executable, "-c", verifier, str(archive_path), str(output.parent)],
                                        capture_output=True, text=True, encoding="utf-8")
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn("Verified:", result.stdout)
                if update_loader:
                    self.assertIn("active AIRK update loader refreshes AIRK automatically", result.stdout)
                    self.assertIn('AIRK 1.8.11 or older, use "Restart Extensions"', result.stdout)
                    self.assertIn("Extensions pane once", result.stdout)
                    self.assertNotIn("Reload each open VS Code window", result.stdout)
                else:
                    self.assertIn("Reload each open VS Code window", result.stdout)
                    self.assertNotIn("refreshes AIRK automatically", result.stdout)


@unittest.skipUnless(PWSH, "PowerShell 7 is unavailable")
class InstallLocalVsixTests(unittest.TestCase):
    def verify(self, changed=None, update_loader=False):
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
                if update_loader:
                    archive.writestr("extension/airk-update.json", "{}")
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
        self.assertIn("Reload each open VS Code window", result.stdout)
        self.assertNotIn("refreshes AIRK automatically", result.stdout)

    def test_update_loader_package_explains_automatic_and_first_transition_paths(self):
        result = self.verify(update_loader=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("active AIRK update loader refreshes AIRK automatically", result.stdout)
        self.assertIn('AIRK 1.8.11 or older, use "Restart Extensions"', result.stdout)
        self.assertIn("Extensions pane once", result.stdout)
        self.assertNotIn("Reload each open VS Code window", result.stdout)
        self.assertNotIn("Developer: Reload Window", result.stdout)

    def test_ui_mismatch_fails_even_when_host_matches(self):
        for name in ("explorerWebview.js", "explorerWebview.css", "workbenchView.js"):
            with self.subTest(asset=name):
                result = self.verify(name)
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn(name, result.stdout + result.stderr)
                self.assertIn("does not match the VSIX", result.stdout + result.stderr)
