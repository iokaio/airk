# SPDX-License-Identifier: Apache-2.0
"""Stage a platform VSIX from a verified native candidate and compiled AIRK modules."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import stat
import subprocess
import zipfile
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parent.parent
EXTENSION = ROOT / "packages" / "vscode"
TARGETS = {"x86_64-pc-windows-msvc": "win32-x64", "x86_64-unknown-linux-gnu": "linux-x64",
           "aarch64-apple-darwin": "darwin-arm64", "x86_64-apple-darwin": "darwin-x64"}
MODULES = ("extension.js", "model.js", "native.js", "scheduler.js", "coverage.js")
ILLUSTRATIONS = ("local-evidence.png", "index-to-context.png")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def safe_name(name):
    parts = name.split("/")
    if not name or "\\" in name or ":" in name or any(p in ("", ".", "..") for p in parts):
        raise ValueError("Unsafe archive path: " + name)


def verify_vsix(archive, expected, executable, platform=None, version=None, pre_release=None, display_name=None):
    with zipfile.ZipFile(archive) as zipped:
        files = [item for item in zipped.infolist() if not item.is_dir()]
        if len(files) > 15_000 or sum(item.file_size for item in files) > 1_073_741_824:
            raise ValueError("VSIX inventory exceeds verification budget")
        names = [item.filename for item in files]
        if len(names) != len(set(names)):
            raise ValueError("Duplicate VSIX archive entry")
        for item in files:
            safe_name(item.filename)
            if stat.S_ISLNK(item.external_attr >> 16):
                raise ValueError("VSIX links are forbidden")
        generated = {"[Content_Types].xml", "extension.vsixmanifest"}
        payload = {name.removeprefix("extension/"): digest(zipped.read(name)) for name in names if name.startswith("extension/")}
        normalized = {("readme.md" if name == "README.md" else "LICENSE.txt" if name == "LICENSE" else name): value for name, value in expected.items()}
        if payload != normalized or set(names) != {"extension/" + name for name in normalized} | generated:
            raise ValueError("VSIX payload differs from the allowlisted stage")
        if executable != "airk.exe" and not (zipped.getinfo("extension/native/" + executable).external_attr >> 16) & stat.S_IXUSR:
            raise ValueError("VSIX lost native executable permission")
        metadata = ET.fromstring(zipped.read("extension.vsixmanifest"))
        identity = next((element for element in metadata.iter()
                         if element.tag.rsplit("}", 1)[-1] == "Identity"), None)
        if identity is None or identity.get("Id") != "airk" or identity.get("Publisher") != "iokaio":
            raise ValueError("VSIX extension identity differs")
        if platform is not None and identity.get("TargetPlatform") != platform:
            raise ValueError("VSIX platform differs")
        if version is not None and identity.get("Version") != version:
            raise ValueError("VSIX version differs")
        if pre_release is not None:
            flag = next((element.get("Value") for element in metadata.iter()
                         if element.tag.rsplit("}", 1)[-1] == "Property"
                         and element.get("Id") == "Microsoft.VisualStudio.Code.PreRelease"), None)
            if (pre_release and flag != "true") or (not pre_release and flag not in (None, "false")):
                raise ValueError("VSIX pre-release flag differs")
        if display_name is not None:
            label = next((element.text for element in metadata.iter()
                          if element.tag.rsplit("}", 1)[-1] == "DisplayName"), None)
            if label != display_name:
                raise ValueError("VSIX display name differs")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument("--native-dir", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--channel", choices=("preview", "stable"), default="preview")
    args = parser.parse_args()
    native = (args.native_dir or ROOT / "artifacts" / "native" / args.target / "publish").resolve()
    output = (args.output or ROOT / "artifacts" / "extension" / args.target).resolve()
    artifacts = (ROOT / "artifacts").resolve()
    if not output.is_relative_to(artifacts) or output == artifacts or not native.is_relative_to(artifacts):
        raise SystemExit("Native input and fresh extension output must be inside artifacts/")
    manifest = json.loads((native / "manifest.json").read_text(encoding="utf-8"))
    package = json.loads((EXTENSION / "package.json").read_text(encoding="utf-8"))
    pre_release = args.channel == "preview"
    if manifest["target"] != args.target or manifest["vsix_target"] != TARGETS[args.target] or manifest["version"] != package["version"]:
        raise SystemExit("Native candidate and extension version/platform differ")
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    if manifest["commit"] != commit or manifest["cargo_lock_sha256"] != digest((ROOT / "Cargo.lock").read_bytes()):
        raise SystemExit("Native candidate and current source/lock identity differ")
    safe_name(manifest["archive"])
    archive = native / manifest["archive"]
    if digest(archive.read_bytes()) != manifest["archive_sha256"]:
        raise SystemExit("Native archive digest mismatch")
    output.mkdir(parents=True, exist_ok=False)
    stage = output / "stage"
    publish = output / "publish"
    stage.mkdir()
    publish.mkdir()
    executable = "airk.exe" if "windows" in args.target else "airk"
    with zipfile.ZipFile(archive) as zipped:
        if len(zipped.namelist()) != len(set(zipped.namelist())) or set(zipped.namelist()) != set(manifest["payload"]):
            raise SystemExit("Native archive inventory differs")
        for name, expected in manifest["payload"].items():
            safe_name(name)
            if name not in (executable, "LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md") and not name.startswith("notices/"):
                raise SystemExit("Unexpected native payload path: " + name)
            data = zipped.read(name)
            if digest(data) != expected:
                raise SystemExit("Native payload digest mismatch: " + name)
            destination = stage / "native" / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
    (stage / "native" / executable).chmod(0o755)
    shutil.copyfile(native / "manifest.json", stage / "native" / "manifest.json")
    for name in MODULES:
        source = EXTENSION / "out" / name
        if source.is_symlink():
            raise SystemExit("Linked runtime module forbidden")
        (stage / "out").mkdir(exist_ok=True)
        shutil.copyfile(source, stage / "out" / name)
    for key in ("scripts", "devDependencies"):
        package.pop(key, None)
    (stage / "package.json").write_text(json.dumps(package, indent=2) + "\n", encoding="utf-8")
    listing = (EXTENSION / "README.md").read_text(encoding="utf-8")
    (stage / "images").mkdir()
    for name in ILLUSTRATIONS:
        source = ROOT / "docs" / "images" / name
        if source.is_symlink() or not source.is_file() or source.stat().st_size > 2_097_152:
            raise SystemExit("Invalid or oversized extension illustration: " + name)
        shutil.copyfile(source, stage / "images" / name)
        listing = listing.replace("https://raw.githubusercontent.com/iokaio/airk/main/docs/images/" + name,
                                  "images/" + name)
    (stage / "README.md").write_text(listing, encoding="utf-8")
    for name in ("LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md"):
        shutil.copyfile(ROOT / name, stage / name)
    expected = {file.relative_to(stage).as_posix(): digest(file.read_bytes()) for file in stage.rglob("*") if file.is_file()}
    vsix = publish / ("airk-" + package["version"] + "-" + TARGETS[args.target] + ".vsix")
    subprocess.run(["node", str(EXTENSION / "scripts" / "package.cjs"), str(stage), TARGETS[args.target], str(vsix), args.channel], cwd=ROOT, check=True)
    verify_vsix(vsix, expected, executable, TARGETS[args.target], package["version"], pre_release, package["displayName"])
    receipt = {"schema": "airk-extension-candidate/1", "version": package["version"], "commit": commit,
               "extension_id": package["publisher"] + "." + package["name"],
               "display_name": package["displayName"], "pre_release": pre_release,
               "platform": TARGETS[args.target], "native_archive_sha256": manifest["archive_sha256"],
               "native_manifest_sha256": digest((native / "manifest.json").read_bytes()),
               "npm_lock_sha256": digest((EXTENSION / "package-lock.json").read_bytes()),
               "node": subprocess.check_output(["node", "--version"], text=True).strip(),
               "vsce": "4.0.0", "archive": vsix.name, "archive_sha256": digest(vsix.read_bytes()),
               "payload": expected, "qualification": "verified package; installed-editor lifecycle qualification separate"}
    receipt_path = publish / "manifest.json"
    receipt_path.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    (publish / "SHA256SUMS").write_text(digest(vsix.read_bytes()) + "  " + vsix.name + "\n" + digest(receipt_path.read_bytes()) + "  manifest.json\n", encoding="utf-8")
    print("Verified platform VSIX: " + str(vsix))


if __name__ == "__main__":
    main()
