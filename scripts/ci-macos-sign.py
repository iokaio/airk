# SPDX-License-Identifier: Apache-2.0
"""Provision an isolated signing keychain only on the manual hosted Mac workflow.

Secrets arrive through step environment variables. security receives passwords
through its interactive stdin interface; notarytool uses its password prompt.
Neither secret values nor captured credential-tool diagnostics are logged.
"""
import argparse
import base64
import os
from pathlib import Path
import secrets
import shlex
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
SECRET_NAMES = ("MACOS_CERTIFICATE_P12", "MACOS_CERTIFICATE_PASSWORD",
                "APPLE_ID", "APPLE_APP_SPECIFIC_PASSWORD")
IDENTITY = "Developer ID Application: Ioka LLC (5PX7D99PXA)"
TEAM = "5PX7D99PXA"


def runner_directory():
    if (sys.platform != "darwin" or os.environ.get("GITHUB_ACTIONS") != "true"
            or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted"
            or os.environ.get("GITHUB_REF") != "refs/heads/main"
            or os.environ.get("GITHUB_EVENT_NAME") != "workflow_dispatch"
            or os.environ.get("GITHUB_WORKFLOW_REF") not in (
                "iokaio/airk/.github/workflows/macos-signing.yml@refs/heads/main",
                "iokaio/airk/.github/workflows/github-release.yml@refs/heads/main")):
        raise ValueError("Signing credentials require the manual main-branch hosted Mac workflow")
    temporary = Path(os.environ["RUNNER_TEMP"]).resolve(strict=True)
    return temporary / "airk-macos-signing"


def private_run(command, *, input=None):
    # Do not propagate GitHub secrets into subprocess environments or exceptions.
    environment = {key: value for key, value in os.environ.items() if key not in SECRET_NAMES}
    result = subprocess.run(command, input=input, capture_output=True, text=True,
                            env=environment, timeout=180, cwd=ROOT)
    if result.returncode:
        raise RuntimeError(f"Credential tool {Path(command[0]).name} failed ({result.returncode}); diagnostics suppressed")
    return result.stdout


def security(*arguments):
    if any(any(character in str(value) for character in "\r\n\0") for value in arguments):
        raise ValueError("Invalid multiline keychain argument")
    return private_run(["/usr/bin/security", "-i"],
                       input=shlex.join(map(str, arguments)) + "\n")


def cleanup(directory):
    if directory.is_symlink():
        raise ValueError("Linked credential directory rejected")
    keychain = directory / "signing.keychain-db"
    if keychain.exists():
        security("delete-keychain", keychain)
    if directory.exists():
        shutil.rmtree(directory)


def sign(target):
    directory = runner_directory()
    values = {name: os.environ.pop(name, "") for name in SECRET_NAMES}
    if not all(values.values()):
        raise ValueError("Required macos-signing environment secrets are missing")
    if any(any(c in value for c in "\r\n\0") for value in values.values()):
        raise ValueError("Signing secrets must be single-line values")
    certificate = base64.b64decode(values["MACOS_CERTIFICATE_P12"], validate=True)
    original_keychains = shlex.split(security("list-keychains", "-d", "user"))
    directory.mkdir(mode=0o700, exist_ok=False)
    keychain = directory / "signing.keychain-db"
    p12 = directory / "certificate.p12"
    try:
        p12.write_bytes(certificate)
        p12.chmod(0o600)
        password = secrets.token_hex(32)
        security("create-keychain", "-p", password, keychain)
        security("set-keychain-settings", "-lut", "3600", keychain)
        security("unlock-keychain", "-p", password, keychain)
        security("import", p12, "-k", keychain, "-P", values["MACOS_CERTIFICATE_PASSWORD"],
                 "-T", "/usr/bin/codesign", "-T", "/usr/bin/security")
        p12.unlink()
        security("set-key-partition-list", "-S", "apple-tool:,apple:", "-s", "-k", password, keychain)
        # codesign's --keychain restricts identity lookup, but certificate-chain
        # resolution also needs the keychain in the user's search list.
        security("list-keychains", "-d", "user", "-s", keychain, *original_keychains)
        identities = security("find-identity", "-v", "-p", "codesigning", keychain)
        if IDENTITY not in identities:
            raise ValueError("The imported keychain lacks the expected Ioka Developer ID identity")
        private_run(["/usr/bin/xcrun", "notarytool", "store-credentials", "airk-ci",
                     "--apple-id", values["APPLE_ID"], "--team-id", TEAM, "--keychain", str(keychain)],
                    input=values["APPLE_APP_SPECIFIC_PASSWORD"] + "\n")
        values.clear()
        subprocess.run([sys.executable, str(ROOT / "scripts/sign-native.py"),
                        "--target", target, "--identity", IDENTITY, "--team-id", TEAM,
                        "--notarize", "--keychain-profile", "airk-ci", "--keychain", str(keychain),
                        "--output", f"artifacts/signed/{target}"], cwd=ROOT, check=True)
    finally:
        try:
            security("list-keychains", "-d", "user", "-s", *original_keychains)
        finally:
            cleanup(directory)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=("aarch64-apple-darwin", "x86_64-apple-darwin"))
    parser.add_argument("--cleanup", action="store_true")
    args = parser.parse_args()
    if args.cleanup:
        cleanup(runner_directory())
    elif args.target:
        sign(args.target)
    else:
        parser.error("Supply --target or --cleanup")


if __name__ == "__main__":
    main()
