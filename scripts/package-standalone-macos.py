# SPDX-License-Identifier: Apache-2.0
"""Build a signed, notarized native-only Mac installer from a verified runtime.

Uses the existing Developer ID Application identity; no Installer certificate,
VSIX, installed service, source directory or OS trust store is touched.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
TARGETS = {"aarch64-apple-darwin": "arm64", "x86_64-apple-darwin": "x86_64"}
TEAM = "5PX7D99PXA"
CERTIFICATE_SHA256 = "4260bd90b8623e45da6d67144a2bff4fd8fa0a8186c7281c0125e39877c3f5c1"


def module(name, filename):
    specification = importlib.util.spec_from_file_location(name, ROOT / "scripts" / filename)
    result = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(result)
    return result


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def run(arguments, timeout=180):
    result = subprocess.run(list(map(str, arguments)), cwd=ROOT, capture_output=True,
                            text=True, encoding="utf-8", errors="replace", timeout=timeout)
    if result.returncode:
        raise RuntimeError(f"{Path(str(arguments[0])).name} failed ({result.returncode}): " + result.stderr[-4096:])
    return result.stdout


def template(filename, **values):
    text = (ROOT / "installers/macos" / filename).read_text(encoding="utf-8")
    for key, value in values.items():
        text = text.replace("@@" + key + "@@", value)
    if "@@" in text:
        raise ValueError("Unresolved Mac installer template value")
    return text.encode("utf-8")


def installer_files(version, target, manifest_sha256, executable_sha256):
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version) or target not in TARGETS:
        raise ValueError("Unsupported standalone installer version or target")
    if any(not re.fullmatch(r"[0-9a-f]{64}", value) for value in (manifest_sha256, executable_sha256)):
        raise ValueError("Installer requires the distributor's exact manifest and executable digests")
    return {
        "Install AIRK.command": template("Install AIRK.command.in", ARCH=TARGETS[target],
                                         MANIFEST_SHA256=manifest_sha256, EXECUTABLE_SHA256=executable_sha256),
        "README.txt": template("README.txt.in", VERSION=version, ARCH=TARGETS[target]),
    }


def stage(runtime, destination, files, executables, wrappers):
    destination.mkdir()
    staged_runtime = destination / "runtime"
    staged_runtime.mkdir()
    for name, expected in files.items():
        source = runtime / name
        if source.is_symlink() or not source.resolve(strict=True).is_relative_to(runtime):
            raise ValueError("Linked or escaped standalone runtime input")
        target = staged_runtime / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
        target.chmod(0o755 if name in executables else 0o644)
        if digest(target) != expected:
            raise ValueError("Native payload changed during standalone staging")
    for name, encoded in wrappers.items():
        (destination / name).write_bytes(encoded)
        (destination / name).chmod(0o755 if name.endswith(".command") else 0o644)


def verify_image_signature(image, identity, team, keychain):
    requirement = (f'=anchor apple generic and certificate leaf[subject.OU] = "{team}" '
                   'and certificate leaf[field.1.2.840.113635.100.6.1.13] exists '
                   f'and certificate leaf = H"{identity}"')
    run(["/usr/bin/codesign", "--verify", "--strict", "--verbose=2", "-R", requirement, image])
    result = subprocess.run(["/usr/bin/codesign", "--display", "--verbose=4", str(image)],
                            capture_output=True, text=True, encoding="utf-8", timeout=30, check=True)
    if ("Authority=Developer ID Application:" not in result.stderr
            or f"TeamIdentifier={team}" not in result.stderr or "Timestamp=" not in result.stderr):
        raise ValueError("Installer lacks the expected Developer ID and trusted timestamp")
    # The same reviewed public leaf pin used by ci-macos-sign.py binds the outer
    # installer too. Only public certificate material enters this temporary path.
    with tempfile.TemporaryDirectory(prefix="airk-installer-certificate-") as temporary:
        prefix = Path(temporary) / "certificate"
        run(["/usr/bin/codesign", "--display", "--extract-certificates=" + str(prefix), image])
        leaf = Path(str(prefix) + "0")
        if (leaf.is_symlink() or not leaf.is_file() or leaf.stat().st_size > 65536
                or digest(leaf) != CERTIFICATE_SHA256
                or hashlib.sha1(leaf.read_bytes()).hexdigest() != identity.lower()):
            raise ValueError("Installer certificate differs from the reviewed signing pin")
    return {"platform": "Darwin", "identity": identity.lower(), "team_id": team,
            "certificate_sha256": CERTIFICATE_SHA256, "trust": "apple_developer_id",
            "timestamp_present": True}


def verify_mounted_payload(image, mount, files, wrappers, package):
    mount.mkdir()
    attached = False
    try:
        run(["/usr/bin/hdiutil", "attach", "-readonly", "-nobrowse", "-mountpoint", mount, image])
        attached = True
        if package.inventory(mount / "runtime") != files:
            raise ValueError("Disk image native payload differs from the verified runtime")
        for name, expected in wrappers.items():
            path = mount / name
            if path.is_symlink() or not path.is_file() or path.read_bytes() != expected:
                raise ValueError("Disk image bootstrap or instructions differ")
        if not (mount / "Install AIRK.command").stat().st_mode & 0o111:
            raise ValueError("Disk image bootstrap is not executable")
    finally:
        if attached or mount.is_mount():
            run(["/usr/bin/hdiutil", "detach", mount])


def package_installer(args):
    if sys.platform != "darwin" or args.target not in TARGETS:
        raise ValueError("Build and verify the installer on its Mac runner")
    if not re.fullmatch(r"[0-9a-fA-F]{40}", args.identity) or args.team_id != TEAM:
        raise ValueError("Use the existing exact Developer ID Application identity and team")
    if not args.notary_profile or not args.keychain:
        raise ValueError("Installer notarization requires the existing explicit keychain profile")
    package = module("standalone_macos_native", "package-native.py")
    signer = module("standalone_macos_signer", "sign-native.py")
    source = args.runtime_dir.resolve(strict=True)
    signed = args.signed_dir.resolve(strict=True)
    descriptor = package.bounded_json(source / "airk-runtime.json", 4096)
    if (descriptor.get("schema") != 2
            or not package.runtime_builder().single_image_version(descriptor.get("version", ""))):
        raise ValueError("Standalone installer requires the native single-executable setup runtime")
    commit = run(["git", "rev-parse", "HEAD"]).strip()
    lock_digest = digest(ROOT / "Cargo.lock")
    runtime, signing = signer.read_verified_pair(signed, source, args.target, commit, lock_digest)
    metadata = package.bounded_json(runtime / "airk-install.json")
    build = package.bounded_json(runtime / "airk-build.json")
    version = metadata["version"]
    files, executables = package.validate_runtime(runtime, version, args.target, build["source"])
    manifest_sha256 = digest(runtime / "airk-install.json")
    wrappers = installer_files(version, args.target, manifest_sha256, files["AIRK.app/Contents/MacOS/airk"])
    output = (args.output or ROOT / "artifacts/standalone" / args.target).resolve()
    artifacts = (ROOT / "artifacts").resolve()
    if not output.is_relative_to(artifacts) or output == artifacts:
        raise ValueError("Standalone output must be a fresh subdirectory of artifacts/")
    output.mkdir(parents=True, exist_ok=False)
    publish = output / "publish"
    publish.mkdir()
    contents = output / "contents"
    stage(runtime, contents, files, executables, wrappers)
    if package.inventory(runtime) != files or package.inventory(contents / "runtime") != files:
        raise ValueError("Signed native input changed during installer staging")
    image = publish / f"airk-{version}-{args.target}-installer.dmg"
    run(["/usr/bin/hdiutil", "create", "-quiet", "-format", "UDZO", "-fs", "HFS+",
         "-volname", f"AIRK {version}", "-srcfolder", contents, image], timeout=600)
    keychain = ["--keychain", str(args.keychain)]
    run(["/usr/bin/codesign", "--sign", args.identity, "--timestamp", *keychain, image])
    verification = verify_image_signature(image, args.identity, args.team_id, args.keychain)
    notarization = json.loads(run(["/usr/bin/xcrun", "notarytool", "submit", image,
        "--keychain-profile", args.notary_profile, *keychain, "--wait", "--timeout", "20m",
        "--output-format", "json"], timeout=1250))
    if notarization.get("status") != "Accepted" or not notarization.get("id"):
        raise RuntimeError("Apple standalone installer notarization did not return Accepted")
    run(["/usr/bin/xcrun", "stapler", "staple", image])
    run(["/usr/bin/xcrun", "stapler", "validate", image])
    verify_image_signature(image, args.identity, args.team_id, args.keychain)
    run(["/usr/sbin/spctl", "--assess", "--type", "open", "--context", "context:primary-signature", image])
    verify_mounted_payload(image, output / "verify-mount", files, wrappers, package)
    if package.inventory(runtime) != files:
        raise ValueError("Installer packaging changed its signed native input")
    verification["notarization"] = {"id": notarization["id"], "status": "Accepted", "stapled": True,
                                    "scope": image.name}
    receipt = {"schema": "airk-standalone-installer/1", "version": version, "target": args.target,
        "commit": commit, "cargo_lock_sha256": lock_digest, "runtime_install_sha256": manifest_sha256,
        "runtime_source": build["source"], "runtime_signing_sha256": digest(signed / "signing.json"),
        "artifact": image.name, "artifact_sha256": digest(image), "installer_kind": "macos_dmg",
        "signing": verification, "runtime_signing": signing,
        "payload": package.inventory(contents),
        "qualification": "signed and notarized installer payload; installed guided setup is a separate acceptance"}
    manifest = publish / "installer.json"
    manifest.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    (publish / "SHA256SUMS").write_text(
        f'{digest(image)}  {image.name}\n{digest(manifest)}  installer.json\n', encoding="utf-8")
    print("Verified native-only standalone installer: " + str(image))
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True, choices=tuple(TARGETS))
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--signed-dir", type=Path, required=True)
    parser.add_argument("--identity", required=True)
    parser.add_argument("--team-id", default=TEAM)
    parser.add_argument("--notary-profile", required=True)
    parser.add_argument("--keychain", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    package_installer(parser.parse_args())


if __name__ == "__main__":
    main()
