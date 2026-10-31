# SPDX-License-Identifier: Apache-2.0
"""Verify downloaded AIRK release assets against their published records.

Read-only consumer check for one platform: SHA256SUMS, release.json, the VSIX
and native candidate manifests, the VSIX payload inventory and the bundled
runtime inventory. It never executes downloaded files. Optional Authenticode
inspection reads signatures only. The chain ends at GitHub's release record
and its producer workflow run; it does not prove bit-for-bit reproducibility.

Download first, into an empty directory, for example:
  gh release download v1.7.3 --dir DIR --pattern SHA256SUMS --pattern release.json \\
     --pattern 'airk-1.7.3-win32-x64.vsix*' --pattern 'airk-1.7.3-x86_64-pc-windows-msvc*'
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import zipfile

PLATFORMS = {"win32-x64": "x86_64-pc-windows-msvc", "linux-x64": "x86_64-unknown-linux-gnu",
             "darwin-x64": "x86_64-apple-darwin", "darwin-arm64": "aarch64-apple-darwin"}


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


class Checks:
    def __init__(self):
        self.passed, self.failed = [], []

    def expect(self, condition, message):
        (self.passed if condition else self.failed).append(message)
        return condition


def verify(directory, version, platform, commit=None, authenticode=False):
    checks = Checks()
    directory = Path(directory)
    target = PLATFORMS[platform]
    vsix = directory / f"airk-{version}-{platform}.vsix"
    vsix_manifest_path = directory / f"airk-{version}-{platform}.vsix-manifest.json"
    native = directory / f"airk-{version}-{target}.zip"
    native_manifest_path = directory / f"airk-{version}-{target}.native-manifest.json"
    sums = {}
    for line in (directory / "SHA256SUMS").read_text(encoding="utf-8").splitlines():
        if line.strip():
            digest, name = line.split(maxsplit=1)
            sums[name.strip().lstrip("*")] = digest
    release = json.loads((directory / "release.json").read_text(encoding="utf-8"))
    checks.expect(sha256_file(directory / "release.json") == sums.get("release.json"), "release.json matches SHA256SUMS")
    checks.expect(release.get("tag") == f"v{version}" and release.get("package_version") == version, "release.json names the requested version")
    for path in (vsix, vsix_manifest_path, native, native_manifest_path):
        actual = sha256_file(path)
        checks.expect(actual == sums.get(path.name), f"{path.name} matches SHA256SUMS")
        checks.expect(actual == release.get("assets", {}).get(path.name), f"{path.name} matches release.json")
    vsix_manifest = json.loads(vsix_manifest_path.read_text(encoding="utf-8"))
    native_manifest = json.loads(native_manifest_path.read_text(encoding="utf-8"))
    for record, label in ((vsix_manifest, "VSIX manifest"), (native_manifest, "native manifest")):
        checks.expect(record.get("commit") == release.get("commit") and record.get("tree") == release.get("tree"),
                      f"{label} commit and tree match release.json")
    if commit:
        checks.expect(release.get("commit") == commit, "release commit matches the expected source commit")
    checks.expect(native_manifest.get("dirty_checkout") is False, "native candidate was built from a clean checkout")
    checks.expect(vsix_manifest.get("archive_sha256") == sha256_file(vsix), "VSIX manifest binds the VSIX bytes")
    checks.expect(vsix_manifest.get("native_archive_sha256") == sha256_file(native)
                  and vsix_manifest.get("native_manifest_sha256") == sha256_file(native_manifest_path),
                  "VSIX manifest binds the native archive and its manifest")
    with zipfile.ZipFile(vsix) as archive:
        names = set(archive.namelist())
        # vsce stores LICENSE as LICENSE.txt and README.md as readme.md. CI manifests keep
        # the staged names, signed promotions the stored ones (as package-extension.py does).
        payload = {("readme.md" if name == "README.md" else "LICENSE.txt" if name == "LICENSE" else name): digest
                   for name, digest in vsix_manifest.get("payload", {}).items()}
        mismatched = [name for name, digest in payload.items()
                      if f"extension/{name}" not in names or hashlib.sha256(archive.read(f"extension/{name}")).hexdigest() != digest]
        extra = sorted(name for name in names if name.startswith("extension/") and name[len("extension/"):] not in payload)
        checks.expect(not mismatched, f"all {len(payload)} VSIX payload files match the VSIX manifest")
        checks.expect(not extra, "the VSIX contains no files outside its manifest")
        wrapper = json.loads(archive.read("extension/native/manifest.json"))
        inventory_bytes = archive.read("extension/native/runtime/airk-install.json")
        checks.expect(wrapper.get("install_manifest_sha256") == hashlib.sha256(inventory_bytes).hexdigest()
                      == native_manifest.get("install_manifest_sha256"),
                      "bundled runtime inventory matches the VSIX wrapper and the native candidate")
        inventory = json.loads(inventory_bytes)
        runtime_mismatch = [name for name, digest in inventory.get("files", {}).items()
                            if hashlib.sha256(archive.read(f"extension/native/runtime/{name}")).hexdigest() != digest]
        checks.expect(not runtime_mismatch, f"all {len(inventory.get('files', {}))} bundled runtime files match their inventory")
        signatures = {}
        if authenticode and platform.startswith("win32"):
            # Extract beside the downloads; some hosts isolate the process temporary directory.
            with tempfile.TemporaryDirectory(dir=directory) as scratch:
                for name in ("airk.exe", "airk-embed.exe"):
                    path = Path(scratch) / name
                    path.write_bytes(archive.read(f"extension/native/runtime/{name}"))
                    # Reads the embedded signature; the file is never executed.
                    # Windows PowerShell started from PowerShell 7 must not inherit its module path.
                    environment = {key: value for key, value in os.environ.items() if key.upper() != "PSMODULEPATH"}
                    powershell = Path(os.environ.get("SystemRoot", r"C:\Windows")) / "System32" / "WindowsPowerShell" / "v1.0" / "powershell.exe"
                    output = subprocess.run([str(powershell), "-NoProfile", "-NonInteractive", "-Command",
                                             f"$s=Get-AuthenticodeSignature -LiteralPath '{path}'; "
                                             "[pscustomobject]@{status=[string]$s.Status;subject=$s.SignerCertificate.Subject} | ConvertTo-Json -Compress"],
                                            capture_output=True, text=True, check=False, env=environment)
                    signatures[name] = json.loads(output.stdout or "{}") or {"error": output.stderr.strip()[:500]}
                    checks.expect(signatures[name].get("status") == "Valid", f"{name} has a valid Authenticode signature")
    return {"version": version, "platform": platform, "commit": release.get("commit"), "tree": release.get("tree"),
            "producer_run": release.get("run_id"), "channel": release.get("channel"),
            "vsix_sha256": sha256_file(vsix), "native_sha256": sha256_file(native),
            "signed_distribution": native_manifest.get("signed_distribution"), "authenticode": signatures,
            "qualification": {"native": native_manifest.get("qualification"), "vsix": vsix_manifest.get("qualification")},
            "passed": checks.passed, "failed": checks.failed,
            "not_verified": ["bit-for-bit reproduction of the binaries from source",
                             f"authenticity of producer run {release.get('run_id')}: confirm it with `gh run view {release.get('run_id')} --repo iokaio/airk`",
                             "installed-editor install, upgrade and uninstall behavior on this platform"]}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--platform", choices=sorted(PLATFORMS), required=True)
    parser.add_argument("--commit", help="expected source commit, for example from `git rev-parse v1.7.3^{commit}`")
    parser.add_argument("--authenticode", action="store_true", help="also read Windows Authenticode signatures")
    args = parser.parse_args(argv)
    result = verify(args.directory, args.version, args.platform, args.commit, args.authenticode)
    print(json.dumps(result, indent=2))
    return 1 if result["failed"] else 0


if __name__ == "__main__":
    sys.exit(main())
