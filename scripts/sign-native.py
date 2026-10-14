# SPDX-License-Identifier: Apache-2.0
"""Sign a fresh artifact copy using an explicit existing OS signing identity.

No certificate import/export, keychain modification or publication. Apple uploads
require --notarize explicitly and use an existing notarytool keychain profile.
"""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import re
import shutil
import subprocess
import tempfile
import urllib.parse
import zipfile

ROOT = Path(__file__).resolve().parent.parent


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(args, timeout=180):
    result = subprocess.run(list(map(str, args)), cwd=ROOT, capture_output=True,
                            text=True, encoding="utf-8", errors="replace", timeout=timeout)
    if result.returncode:
        raise RuntimeError(f"{Path(str(args[0])).name} failed ({result.returncode}): " + result.stderr[-4096:])
    return result.stdout


def verify(binary, receipt, signtool="signtool"):
    if receipt["platform"] != platform.system() or digest(binary) != receipt["binary_sha256"]:
        raise ValueError("Signing receipt platform or executable digest differs")
    if platform.system() == "Windows":
        if not re.fullmatch(r"[0-9a-fA-F]{40}", receipt["identity"]):
            raise ValueError("Expected exact certificate SHA-1 thumbprint")
        run([signtool, "verify", "/pa", "/all", "/v", binary])
        identity = json.loads(run(["pwsh", "-NoProfile", "-NonInteractive", "-File",
                                   ROOT / "scripts/verify-authenticode.ps1", "-Binary", binary,
                                   "-Thumbprint", receipt["identity"]]))
        return {"trust":"authenticode_valid", "timestamp_present":True,
                "signer_thumbprint":identity["thumbprint"]}
    if platform.system() == "Darwin":
        team = receipt["team_id"]
        if not re.fullmatch(r"[A-Z0-9]{10}", team):
            raise ValueError("Expected exact Apple team identifier")
        run(["/usr/bin/codesign", "--verify", "--strict", "--verbose=2", "-R",
             f'=anchor apple generic and certificate leaf[subject.OU] = "{team}" and certificate leaf[field.1.2.840.113635.100.6.1.13] exists', binary])
        result = subprocess.run(["/usr/bin/codesign", "--display", "--verbose=4", str(binary)],
                                capture_output=True, text=True, encoding="utf-8", timeout=30, check=True)
        details = result.stderr
        if ("Authority=Developer ID Application:" not in details or not re.search(r"flags=0x[0-9a-f]+\([^\n)]*\bruntime\b",details)
                or f"TeamIdentifier={team}" not in details or "Timestamp=" not in details):
            raise ValueError("Developer ID, hardened runtime or trusted timestamp is absent")
        return {"trust":"apple_developer_id", "team_id":team, "hardened_runtime":True,
                "timestamp_present":True}
    raise ValueError("This signer supports Windows and macOS native executables")


def read_verified(directory, source, target, commit, lock_digest, signtool="signtool"):
    receipt_path = directory / "signing.json"
    if receipt_path.is_symlink() or receipt_path.stat().st_size > 16_384:
        raise ValueError("Linked or oversized signing receipt")
    receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    if (receipt.get("schema") != "airk-native-signing/1" or receipt["commit"] != commit
            or receipt["target"] != target or receipt["cargo_lock_sha256"] != lock_digest
            or receipt["source_sha256"] != digest(source)):
        raise ValueError("Signing receipt differs from the selected build")
    binary = directory / source.name
    if binary.is_symlink():
        raise ValueError("Linked signed executable rejected")
    receipt["verification"] = verify(binary, receipt, signtool)
    return binary, receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True, choices=("x86_64-pc-windows-msvc", "x86_64-apple-darwin", "aarch64-apple-darwin"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--identity", required=True, help="Windows certificate thumbprint or Apple Developer ID Application identity")
    parser.add_argument("--signtool", default="signtool")
    parser.add_argument("--timestamp-url")
    parser.add_argument("--team-id")
    parser.add_argument("--notarize", action="store_true")
    parser.add_argument("--keychain-profile")
    parser.add_argument("--keychain", type=Path, help="Explicit existing Apple signing/notary keychain (for isolated CI keychains)")
    args = parser.parse_args()
    windows = "windows" in args.target
    if platform.system() != ("Windows" if windows else "Darwin"):
        parser.error("Sign and verify on the intended native OS")
    if windows:
        url = urllib.parse.urlsplit(args.timestamp_url or "")
        if not re.fullmatch(r"[0-9a-fA-F]{40}", args.identity) or url.scheme not in ("http", "https") or not url.hostname or url.username or url.password:
            parser.error("Windows signing requires an exact certificate thumbprint and explicit RFC3161 timestamp URL")
        if args.notarize or args.keychain_profile or args.team_id or args.keychain:
            parser.error("Apple signing options do not apply to Windows")
    elif not args.identity.startswith("Developer ID Application:") or not re.fullmatch(r"[A-Z0-9]{10}", args.team_id or ""):
        parser.error("Apple signing requires a Developer ID Application identity and exact team ID")
    if args.notarize and not args.keychain_profile:
        parser.error("Notarization requires an existing notarytool keychain profile")
    source = ROOT / "target" / args.target / "release" / ("airk.exe" if windows else "airk")
    if source.is_symlink() or not source.is_file():
        parser.error("Build the target release binary first")
    output = args.output.resolve()
    if not output.is_relative_to((ROOT / "artifacts").resolve()) or output == (ROOT / "artifacts").resolve():
        parser.error("Signing output must be a fresh directory inside artifacts/")
    output.mkdir(parents=True, exist_ok=False)
    binary = output / source.name
    shutil.copyfile(source, binary)
    binary.chmod(0o755)
    receipt = {"schema":"airk-native-signing/1", "platform":platform.system(), "target":args.target,
               "identity":args.identity, "team_id":args.team_id, "source_sha256":digest(source),
               "commit":run(["git","rev-parse","HEAD"]).strip(), "cargo_lock_sha256":digest(ROOT/"Cargo.lock"),
               "notarization":None}
    if windows:
        run([args.signtool,"sign","/sha1",args.identity,"/fd","SHA256","/tr",args.timestamp_url,"/td","SHA256",binary])
    else:
        keychain = ["--keychain", str(args.keychain)] if args.keychain else []
        run(["/usr/bin/codesign","--sign",args.identity,"--options","runtime","--timestamp",*keychain,binary])
    receipt["binary_sha256"] = digest(binary)
    receipt["verification"] = verify(binary,receipt,args.signtool)
    if args.notarize:
        # A standalone CLI cannot carry a stapled ticket. The server-side ticket
        # binds this signed binary; offline Gatekeeper qualification stays explicit.
        with tempfile.TemporaryDirectory(prefix="airk-notary-") as temporary:
            archive = Path(temporary)/"airk.zip"
            with zipfile.ZipFile(archive,"x",compression=zipfile.ZIP_DEFLATED) as zipped:
                zipped.write(binary,binary.name)
            result = json.loads(run(["/usr/bin/xcrun","notarytool","submit",archive,"--keychain-profile",
                                     args.keychain_profile,*keychain,"--wait","--timeout","20m","--output-format","json"],timeout=1250))
            if result.get("status") != "Accepted" or not result.get("id"):
                raise RuntimeError("Apple notarization did not return Accepted")
            receipt["notarization"] = {"id":result["id"],"status":"Accepted","stapled":False}
    # A failed sign/verification/notarization never produces a success receipt.
    (output/"signing.json").write_text(json.dumps(receipt,indent=2)+"\n",encoding="utf-8")
    print(f"Verified signed native copy: {binary}")


if __name__ == "__main__":
    main()
