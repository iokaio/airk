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

Split releases (1.8.24 and later; scripts/split_receipts.py) carry one universal VSIX with no
native code, the airk + airkctl pair ZIP per target and, on stable, the AIRK Service installer:
  gh release download v1.8.24 --dir DIR --pattern SHA256SUMS --pattern release.json \\
     --pattern 'airk-1.8.24.vsix*' --pattern 'airk-1.8.24-x86_64-pc-windows-msvc*'
The installer is checked against its manifest, the pair and (Linux) every archive member.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import importlib.util
import sys
import tarfile
import tempfile
import zipfile

PLATFORMS = {"win32-x64": "x86_64-pc-windows-msvc", "linux-x64": "x86_64-unknown-linux-gnu",
             "darwin-x64": "x86_64-apple-darwin", "darwin-arm64": "aarch64-apple-darwin"}
# The C7 owner of the split threshold and receipt validators, beside this file.
_SPEC = importlib.util.spec_from_file_location("published_split_receipts", Path(__file__).with_name("split_receipts.py"))
receipts = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(receipts)
INSTALLERS = {"win32-x64": "exe", "linux-x64": "tar.gz", "darwin-x64": "dmg", "darwin-arm64": "dmg"}
# Split plan §9.5: where each tested image is placed inside the installer's payload/ folder.
SERVICE_PATHS = {"win32-x64": {"airk.exe": "payload/service/airk.exe", "airkctl.exe": "payload/bin/airkctl.exe"},
                 "linux-x64": {"airk": "payload/service/airk", "airkctl": "payload/bin/airkctl"},
                 "darwin-x64": {name: "payload/" + name for name in receipts.IMAGES["macos"].values()},
                 "darwin-arm64": {name: "payload/" + name for name in receipts.IMAGES["macos"].values()}}


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


def read_sums(directory):
    sums = {}
    for line in (directory / "SHA256SUMS").read_text(encoding="utf-8").splitlines():
        if line.strip():
            digest, name = line.split(maxsplit=1)
            sums[name.strip().lstrip("*")] = digest
    return sums


def authenticode_status(path):
    """Reads the embedded signature; the file is never executed."""
    # Windows PowerShell started from PowerShell 7 must not inherit its module path.
    environment = {key: value for key, value in os.environ.items() if key.upper() != "PSMODULEPATH"}
    powershell = Path(os.environ.get("SystemRoot", r"C:\Windows")) / "System32" / "WindowsPowerShell" / "v1.0" / "powershell.exe"
    output = subprocess.run([str(powershell), "-NoProfile", "-NonInteractive", "-Command",
                             f"$s=Get-AuthenticodeSignature -LiteralPath '{path}'; "
                             "[pscustomobject]@{status=[string]$s.Status;subject=$s.SignerCertificate.Subject} | ConvertTo-Json -Compress"],
                            capture_output=True, text=True, check=False, env=environment)
    return json.loads(output.stdout or "{}") or {"error": output.stderr.strip()[:500]}


def accepted(validator, document):
    try:
        validator(document)
    except ValueError:
        return False
    return True


def verify_split(directory, version, platform, commit, authenticode):
    """Split catalog: the universal VSIX, this platform's pair ZIP and, when published, its installer."""
    checks = Checks()
    target = PLATFORMS[platform]
    vsix = directory / f"airk-{version}.vsix"
    vsix_manifest_path = directory / f"airk-{version}.vsix-manifest.json"
    native = directory / f"airk-{version}-{target}.zip"
    native_manifest_path = directory / f"airk-{version}-{target}.native-manifest.json"
    installer = directory / f"airk-{version}-{target}-installer.{INSTALLERS[platform]}"
    installer_manifest_path = directory / f"airk-{version}-{target}-installer.manifest.json"
    sums = read_sums(directory)
    release = json.loads((directory / "release.json").read_text(encoding="utf-8"))
    checks.expect(sha256_file(directory / "release.json") == sums.get("release.json"), "release.json matches SHA256SUMS")
    checks.expect(release.get("tag") == f"v{version}" and release.get("package_version") == version, "release.json names the requested version")
    if commit:
        checks.expect(release.get("commit") == commit, "release commit matches the expected source commit")
    assets = release.get("assets", {})
    published_installer = installer.name in assets
    checks.expect(published_installer == (release.get("channel") == "stable"),
                  "the release publishes an installer for this platform exactly when its channel requires one")
    paths = [vsix, vsix_manifest_path, native, native_manifest_path]
    if published_installer:
        paths += [installer, installer_manifest_path]
    for path in paths:
        actual = sha256_file(path)
        checks.expect(actual == sums.get(path.name), f"{path.name} matches SHA256SUMS")
        checks.expect(actual == assets.get(path.name), f"{path.name} matches release.json")
    vsix_manifest = json.loads(vsix_manifest_path.read_text(encoding="utf-8"))
    native_manifest = json.loads(native_manifest_path.read_text(encoding="utf-8"))
    for record, label in ((vsix_manifest, "VSIX manifest"), (native_manifest, "native manifest")):
        checks.expect(record.get("commit") == release.get("commit") and record.get("tree") == release.get("tree"),
                      f"{label} commit and tree match release.json")
    checks.expect(accepted(receipts.validate_vsix, vsix_manifest), "VSIX manifest is a universal VSIX receipt without native entries")
    checks.expect(accepted(receipts.validate_native_candidate, native_manifest), "native manifest is a split service pair receipt")
    checks.expect(native_manifest.get("target") == target and native_manifest.get("dirty_checkout") is False,
                  "native pair was built for this platform from a clean checkout")
    checks.expect(vsix_manifest.get("archive_sha256") == sha256_file(vsix), "VSIX manifest binds the VSIX bytes")
    checks.expect(native_manifest.get("archive_sha256") == sha256_file(native), "native manifest binds the pair ZIP bytes")
    with zipfile.ZipFile(vsix) as archive:
        names = set(archive.namelist())
        payload = {("readme.md" if name == "README.md" else "LICENSE.txt" if name == "LICENSE" else name): digest
                   for name, digest in vsix_manifest.get("payload", {}).items()}
        mismatched = [name for name, digest in payload.items()
                      if f"extension/{name}" not in names or hashlib.sha256(archive.read(f"extension/{name}")).hexdigest() != digest]
        extra = sorted(name for name in names if name.startswith("extension/") and name[len("extension/"):] not in payload)
        native_entries = [name for name in names if name.startswith("extension/native/")
                          or name.lower().endswith(receipts.NATIVE_SUFFIXES)]
        checks.expect(not mismatched, f"all {len(payload)} VSIX payload files match the VSIX manifest")
        checks.expect(not extra, "the VSIX contains no files outside its manifest")
        checks.expect(not native_entries, "the VSIX contains no native code")
    images = receipts.IMAGES["windows" if platform.startswith("win32") else "macos" if platform.startswith("darwin") else "linux"]
    signatures = {}
    with zipfile.ZipFile(native) as archive:
        inventory_bytes = archive.read("airk-install.json")
        checks.expect(hashlib.sha256(inventory_bytes).hexdigest() == native_manifest.get("install_manifest_sha256"),
                      "pair install inventory matches the native manifest")
        inventory = json.loads(inventory_bytes).get("files", {})
        names = {name for name in archive.namelist() if not name.endswith("/")}
        mismatched = [name for name, digest in inventory.items()
                      if name not in names or hashlib.sha256(archive.read(name)).hexdigest() != digest]
        checks.expect(not mismatched and names == set(inventory) | {"airk-install.json"},
                      f"all {len(inventory)} pair files match their install inventory, with nothing else")
        checks.expect((inventory.get(images["airk"]), inventory.get(images["airkctl"]))
                      == (native_manifest.get("airk_sha256"), native_manifest.get("airkctl_sha256")),
                      "both images (airk and airkctl) match the native manifest")
        if authenticode and platform.startswith("win32"):
            # Extract beside the downloads; some hosts isolate the process temporary directory.
            with tempfile.TemporaryDirectory(dir=directory) as scratch:
                for name in images.values():
                    path = Path(scratch) / name
                    path.write_bytes(archive.read(name))
                    signatures[name] = authenticode_status(path)
                    checks.expect(signatures[name].get("status") == "Valid", f"{name} has a valid Authenticode signature")
    installer_sha256 = None
    if published_installer:
        installer_sha256 = sha256_file(installer)
        manifest = json.loads(installer_manifest_path.read_text(encoding="utf-8"))
        roster = manifest.get("payload", {})
        checks.expect(manifest.get("artifact") == installer.name and manifest.get("artifact_sha256") == installer_sha256,
                      "installer manifest binds the installer bytes")
        checks.expect(manifest.get("version") == version and manifest.get("target") == target
                      and manifest.get("commit") == release.get("commit"),
                      "installer manifest names this version, platform and commit")
        checks.expect(manifest.get("runtime_install_sha256") == native_manifest.get("install_manifest_sha256")
                      and manifest.get("runtime_source") == native_manifest.get("source"),
                      "installer was packaged from this release's service pair")
        checks.expect(manifest.get("payload_schema") == "airk-service-payload/1"
                      and manifest.get("payload_inventory_sha256") == roster.get("payload/payload-inventory.json"),
                      "installer pins its service payload inventory")
        checks.expect(all(roster.get(placed) == inventory.get(name) for name, placed in SERVICE_PATHS[platform].items()),
                      "installer places both tested images")
        if platform.startswith("linux"):
            checks.expect(manifest.get("signing") == {"platform": "Linux", "trust": "unsigned"},
                          "Linux installer is labeled unsigned")
            observed, prefix = {}, installer.name.removesuffix(".tar.gz") + "/"
            with tarfile.open(installer, "r|gz") as packaged:
                for member in packaged:
                    if not member.isfile() or not member.name.startswith(prefix) or len(observed) >= 8192:
                        observed = None
                        break
                    observed[member.name[len(prefix):]] = hashlib.file_digest(packaged.extractfile(member), "sha256").hexdigest()
            checks.expect(observed == roster, f"all {len(roster)} installer archive files match the installer manifest")
        elif authenticode and platform.startswith("win32"):
            signatures[installer.name] = authenticode_status(installer)
            checks.expect(signatures[installer.name].get("status") == "Valid", "the installer has a valid Authenticode signature")
    not_verified = ["bit-for-bit reproduction of the binaries from source",
                    f"authenticity of producer run {release.get('run_id')}: confirm it with `gh run view {release.get('run_id')} --repo iokaio/airk`",
                    "AIRK Service install, upgrade and uninstall behavior on this platform"]
    if published_installer and not platform.startswith("linux"):
        not_verified.append("files inside the Windows/macOS installer beyond its signed manifest (open it on that OS)")
    return {"version": version, "platform": platform, "commit": release.get("commit"), "tree": release.get("tree"),
            "producer_run": release.get("run_id"), "channel": release.get("channel"),
            "vsix_sha256": sha256_file(vsix), "native_sha256": sha256_file(native), "installer_sha256": installer_sha256,
            "signed_distribution": native_manifest.get("signed_distribution"), "authenticode": signatures,
            "qualification": {"native": native_manifest.get("qualification"), "vsix": vsix_manifest.get("qualification")},
            "passed": checks.passed, "failed": checks.failed, "not_verified": not_verified}


def verify(directory, version, platform, commit=None, authenticode=False):
    directory = Path(directory)
    if receipts.split_version(version):
        return verify_split(directory, version, platform, commit, authenticode)
    checks = Checks()
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
