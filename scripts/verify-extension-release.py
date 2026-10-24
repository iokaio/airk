# SPDX-License-Identifier: Apache-2.0
"""Revalidate all platform VSIX receipts before granting them to the publisher."""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import zipfile

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("package_extension", ROOT / "scripts/package-extension.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


def read_receipt(path):
    if path.is_symlink() or path.stat().st_size > 4_194_304:
        raise ValueError("Linked or oversized release receipt")
    return json.loads(path.read_text(encoding="utf-8"))


def verify(directory, commit, version, channel, *, receipts=None):
    if not re.fullmatch(r"[0-9a-f]{40}", commit) or not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError("Expected full commit SHA and numeric extension version")
    if channel not in ("preview", "stable"):
        raise ValueError("Unknown release channel")
    platforms = set(package.TARGETS.values())
    archives = []
    receipts = sorted(directory.glob("*/manifest.json")) if receipts is None else receipts
    if len(receipts) != len(platforms):
        raise ValueError("Expected exactly four platform receipts")
    lock_digest = package.digest((ROOT / "packages/vscode/package-lock.json").read_bytes())
    toolchain_digest = package.digest((ROOT / "rust-toolchain.toml").read_bytes())
    targets = {platform: target for target, platform in package.TARGETS.items()}
    for receipt_path in receipts:
        receipt = read_receipt(receipt_path)
        platform = receipt["platform"]
        if platform not in platforms:
            raise ValueError("Unexpected or duplicate release platform")
        platforms.remove(platform)
        if (receipt["schema"] != "airk-extension-candidate/1" or receipt["commit"] != commit
                or receipt["version"] != version or receipt["extension_id"] != "iokaio.airk"
                or receipt["pre_release"] is not (channel == "preview")
                or receipt["npm_lock_sha256"] != lock_digest):
            raise ValueError("Release identity/channel/lock differs")
        name = receipt["archive"]
        package.safe_name(name)
        if "/" in name or name != f"airk-{version}-{platform}.vsix":
            raise ValueError("Unexpected release archive name")
        archive = receipt_path.parent / name
        if archive.is_symlink() or package.digest(archive.read_bytes()) != receipt["archive_sha256"]:
            raise ValueError("Release archive digest mismatch")
        executable = "airk.exe" if platform == "win32-x64" else "airk"
        package.verify_vsix(archive, receipt["payload"], executable, platform, version,
                            channel == "preview", receipt["display_name"])
        with zipfile.ZipFile(archive) as zipped:
            paired = tuple(int(part) for part in version.split(".")[:2]) >= (1, 4)
            native_bytes = zipped.read("extension/native/candidate.json" if paired else "extension/native/manifest.json")
            native = json.loads(native_bytes)
            if (package.digest(native_bytes) != receipt["native_manifest_sha256"]
                    or native.get("schema") != ("airk-native-candidate/2" if paired else "airk-native-candidate/1")
                    or native.get("target") != targets[platform]
                    or native.get("toolchain_file_sha256") != toolchain_digest
                    or native["commit"] != commit or native["version"] != version
                    or native["vsix_target"] != platform or native["dirty_checkout"]
                    or native["archive_sha256"] != receipt["native_archive_sha256"]
                    or native["cargo_lock_sha256"] != package.digest((ROOT / "Cargo.lock").read_bytes())):
                raise ValueError("Embedded native candidate provenance differs or checkout is dirty")
            prefix = "extension/native/runtime/" if paired else "extension/native/"
            for entry, expected in native["payload"].items():
                package.safe_name(entry)
                if package.digest(zipped.read(prefix + entry)) != expected:
                    raise ValueError("Embedded native payload digest mismatch")
            if paired:
                helper = "airk-embed.exe" if platform == "win32-x64" else "airk-embed"
                wrapper = json.loads(zipped.read("extension/native/manifest.json"))
                expected = {"schema": 2, "version": version, "vsix_target": platform, "runtime_dir": "runtime",
                            "install_manifest_sha256": native["payload"]["airk-install.json"],
                            "payload": {name: native["payload"][name] for name in (executable, helper)}}
                if wrapper != expected or native.get("install_manifest_sha256") != expected["install_manifest_sha256"]:
                    raise ValueError("Embedded service wrapper differs from native candidate")
                install = json.loads(zipped.read(prefix + "airk-install.json"))
                if install.get("files") != {name: value for name, value in native["payload"].items() if name != "airk-install.json"}:
                    raise ValueError("Embedded installer inventory differs from complete native pair")
        archives.append(str(archive.resolve()))
    return archives


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--channel", choices=("preview", "stable"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    archives = verify(args.directory, args.commit, args.version, args.channel)
    args.output.write_text(json.dumps(archives, indent=2) + "\n", encoding="utf-8")
    print(f"Verified all {len(archives)} VSIX packages for {args.version} ({args.channel}) at {args.commit}")


if __name__ == "__main__":
    main()
