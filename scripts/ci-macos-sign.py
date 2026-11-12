# SPDX-License-Identifier: Apache-2.0
"""Provision an isolated signing keychain only on the manual hosted Mac workflow.

Secrets arrive through step environment variables. security receives passwords
through its interactive stdin interface; notarytool uses its password prompt.
Neither secret values nor captured credential-tool diagnostics are logged.
"""
import argparse
import base64
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import secrets
import re
import shlex
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
SECRET_NAMES = ("MACOS_CERTIFICATE_P12", "MACOS_CERTIFICATE_PASSWORD",
                "APPLE_ID", "APPLE_APP_SPECIFIC_PASSWORD")
IDENTITY = "Developer ID Application: Ioka LLC (5PX7D99PXA)"
TEAM = "5PX7D99PXA"
# Public leaf certificate observed on the independently verified Neo app and
# standalone launcher. Renewal/rotation requires a reviewed pin and P12 update.
CERTIFICATE_SHA256 = "4260bd90b8623e45da6d67144a2bff4fd8fa0a8186c7281c0125e39877c3f5c1"
# The split threshold's one owner (C7), loaded beside this file.
_RECEIPTS = importlib.util.spec_from_file_location("ci_macos_split_receipts", Path(__file__).with_name("split_receipts.py"))
receipts = importlib.util.module_from_spec(_RECEIPTS)
_RECEIPTS.loader.exec_module(receipts)


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


def imported_identity(keychain):
    """Match a public certificate to its valid private-key identity, not its CN alone."""
    pem = security("find-certificate", "-a", "-c", IDENTITY, "-p", keychain)
    if len(pem) > 262_144:
        raise ValueError("Imported signing certificate is missing or ambiguous")
    certificates = re.findall(r"-----BEGIN CERTIFICATE-----\s+([A-Za-z0-9+/=\s]+?)\s+-----END CERTIFICATE-----", pem)
    if len(certificates) != 1:
        raise ValueError("Imported signing certificate is missing or ambiguous")
    der = base64.b64decode(re.sub(r"\s+", "", certificates[0]), validate=True)
    if len(der) > 65_536 or hashlib.sha256(der).hexdigest() != CERTIFICATE_SHA256:
        raise ValueError("Imported signing certificate does not match the reviewed SHA-256 pin")
    # Apple uses the full SHA-1 certificate hash as an unambiguous identity
    # selector. The policy pin and post-sign leaf check use SHA-256.
    sha1 = hashlib.sha1(der).hexdigest()
    identities = security("find-identity", "-v", "-p", "codesigning", keychain)
    rows = re.findall(r'^\s*\d+\)\s+([A-Fa-f0-9]{40})\s+"([^"\r\n]+)"\s*$', identities, re.MULTILINE)
    counts = re.findall(r'^\s*(\d+) valid identities found\s*$', identities, re.MULTILINE)
    matching = [(value.lower(), name) for value, name in rows if name == IDENTITY or value.lower() == sha1]
    if (len(identities) > 262_144 or len(counts) != 1 or int(counts[0]) != len(rows)
            or matching != [(sha1, IDENTITY)]):
        raise ValueError("The pinned certificate lacks one unambiguous valid signing identity")
    return sha1


def verify_signed_certificate(binary, sha1):
    """Read only the public leaf of the actual signed output; export no private key."""
    with tempfile.TemporaryDirectory(prefix="airk-ci-public-certificate-") as temporary:
        prefix = Path(temporary) / "certificate"
        private_run(["/usr/bin/codesign", "--display", "--extract-certificates=" + str(prefix), str(binary)])
        leaf = Path(str(prefix) + "0")
        if leaf.is_symlink() or not leaf.is_file() or leaf.stat().st_size > 65_536:
            raise ValueError("Signed output lacks a bounded public leaf certificate")
        der = leaf.read_bytes()
        if (hashlib.sha256(der).hexdigest() != CERTIFICATE_SHA256
                or hashlib.sha1(der).hexdigest() != sha1):
            raise ValueError("Signed output certificate differs from the reviewed signing identity")


def verify_signed_output(output, paired, sha1):
    receipt_path = output / "signing.json"
    if receipt_path.is_symlink() or receipt_path.stat().st_size > 65_536:
        raise ValueError("Invalid signed-output receipt")
    receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    if paired:
        components = receipt.get("components")
        if receipt.get("schema") != "airk-service-pair-signing/1" or not isinstance(components, dict) or not 1 <= len(components) <= 8:
            raise ValueError("Invalid signed-pair certificate audit input")
        if any(not isinstance(name, str) or not re.fullmatch(
                r"AIRK\.app/Contents/MacOS/(?:airk|airkctl|airk-embed|airk-launcher-image|libonnxruntime\.dylib)", name)
                for name in components):
            raise ValueError("Invalid signed-pair certificate audit component")
        paths = ["runtime/" + name for name in components] + ["runtime/AIRK.app"]
    else:
        paths = ["airk"]
    root = output.resolve(strict=True)
    for name in paths:
        binary = output / name
        if binary.is_symlink() or not binary.resolve(strict=True).is_relative_to(root):
            raise ValueError("Signed certificate audit path escapes its output")
        verify_signed_certificate(binary, sha1)
    audit = {"schema": "airk-macos-certificate-identity/1", "identity": IDENTITY, "team_id": TEAM,
             "certificate_sha256": CERTIFICATE_SHA256, "certificate_sha1": sha1, "signed_paths": paths}
    # Keep this public audit outside runtime/: no signed payload or installer
    # inventory changes, and no credentials or private key material are retained.
    (output / "certificate-identity.json").write_text(json.dumps(audit, indent=2) + "\n", encoding="utf-8")
    print("Verified CI signing certificate: " + json.dumps(audit, sort_keys=True))
    return audit


def probe_diagnostics(stderr):
    """Bounded traceback coordinates/codes, without arbitrary child text."""
    frames = re.findall(r'File "[^"\r\n]*[/\\]([A-Za-z0-9_.-]+\.py)", line ([0-9]{1,7}), in ([A-Za-z_][A-Za-z0-9_]*)', stderr)
    kinds = re.findall(r'^([A-Za-z_][A-Za-z0-9_]*(?:Error|Exception))(?::|$)', stderr, re.MULTILINE)
    codes = re.findall(r'\b([a-z][a-z0-9_]{1,100}_(?:failed|changed|unavailable|pending|refused|unsupported|invalid))\b', stderr)
    return {"exception_types": sorted(set(kinds))[:16], "native_error_codes": sorted(set(codes))[:16],
            "frames": [{"file": file, "line": int(line), "function": function} for file, line, function in frames[-16:]]}


def qualify_signed_runtime(output, paired, target):
    """Probe the actual signed schema-2 bundle after credential cleanup.

    Historical standalone/pair formats retain their existing signing checks.
    No executable is extracted from its app, re-signed, or written in place.
    """
    if not paired:
        return None
    if any(name in os.environ for name in SECRET_NAMES):
        raise ValueError("Post-sign execution requires signing secrets to be removed first")
    spec = importlib.util.spec_from_file_location("post_sign_native_package", ROOT / "scripts/package-native.py")
    package = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(package)
    runtime = output / "runtime"
    descriptor = package.bounded_json(runtime / "airk-runtime.json", 4096)
    builder = package.runtime_builder()
    version = descriptor.get("version")
    if not isinstance(version, str):
        raise ValueError("Signed runtime probe version is missing")
    modern = builder.single_image_version(version)
    if descriptor.get("schema") == 1 and not modern:
        return None
    split = receipts.split_version(version)
    if descriptor.get("schema") != (3 if split else 2):
        raise ValueError("Unsupported signed runtime schema for installation probes")
    if not modern or "apple" not in target:
        raise ValueError("Signed single-image probes require a modern Mac runtime")
    files, _ = package.validate_runtime(runtime, version, target, builder.source_identity())
    if split:
        return qualify_signed_pair(output, runtime, version, target, package, builder, files)
    report = {"schema": "airk-macos-post-sign-validation/1", "target": target, "version": version,
              "install_manifest_sha256": package.sha256(runtime / "airk-install.json"),
              "executable_sha256": files["AIRK.app/Contents/MacOS/airk"],
              "scope": "signed embedded-model and owned private installation; GUI approval and agent trust are separate",
              "stages": []}
    receipt = output / "post-sign-validation.json"
    def save():
        receipt.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    save()
    stage = "signed_embedded_model"
    try:
        # Uses the same numerical/profile contract as the original build probe,
        # now executing the signed CFBundleExecutable in its sealed app.
        report["model_smoke"] = builder.verify_model_smoke(runtime, "macos", version)
        report["stages"].append("signed_embedded_model_passed")
        save()
        stage = "signed_private_installation"
        environment = {key: value for key, value in os.environ.items() if key not in SECRET_NAMES}
        result = subprocess.run([sys.executable, str(ROOT / "scripts/check-service-install.py"),
                                 "--runtime", str(runtime)], cwd=ROOT, env=environment,
                                capture_output=True, text=True, encoding="utf-8", errors="replace")
        report["service_install"] = {"exit_code": result.returncode,
            "stdout_sha256": hashlib.sha256(result.stdout.encode()).hexdigest(),
            "stderr_sha256": hashlib.sha256(result.stderr.encode()).hexdigest(),
            "diagnostics": probe_diagnostics(result.stderr)}
        if result.returncode:
            raise RuntimeError("Signed Mac private service-install probe failed; owned fixture is retained by the probe")
        report["stages"].append("signed_private_installation_passed")
        stage = "signed_payload_inventory"
        if package.inventory(runtime) != files:
            raise RuntimeError("Signed payload changed during post-sign probes")
        report["status"] = "passed"
    except Exception as error:
        # Do not copy arbitrary child output or secrets into a signing receipt.
        report["status"] = "failed"
        report["failure_stage"] = stage
        report["failure_type"] = type(error).__name__
        save()
        print("Signed Mac runtime execution failed: " + json.dumps(report, sort_keys=True))
        raise
    save()
    print("Verified signed Mac runtime execution: " + json.dumps(report, sort_keys=True))
    return report


def qualify_signed_pair(output, runtime, version, target, package, builder, files):
    """Split pair (schema 3): execute the signed service image's embedded-model probe and the
    nested signed airkctl's --version inside the sealed app. Installation belongs to the setup
    role through the installer (Install AIRK.command); check-service-install.py still drives the
    pre-split `airk` user commands (ci.yml harness row), so it is not run on a split pair here."""
    cli = "AIRK.app/Contents/MacOS/airkctl"
    report = {"schema": "airk-macos-post-sign-validation/1", "target": target, "version": version,
              "install_manifest_sha256": package.sha256(runtime / "airk-install.json"),
              "executable_sha256": files["AIRK.app/Contents/MacOS/airk"], "airkctl_sha256": files[cli],
              "scope": "signed embedded-model and nested airkctl execution; installation, GUI approval and agent trust are separate",
              "stages": []}
    receipt = output / "post-sign-validation.json"
    def save():
        receipt.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    save()
    stage = "signed_embedded_model"
    try:
        report["model_smoke"] = builder.verify_model_smoke(runtime, "macos", version)
        report["stages"].append("signed_embedded_model_passed")
        save()
        stage = "signed_airkctl_version"
        environment = {key: value for key, value in os.environ.items() if key not in SECRET_NAMES}
        result = subprocess.run([str(runtime / cli), "--version"], cwd=ROOT, env=environment, capture_output=True,
                                text=True, encoding="utf-8", errors="replace", timeout=30)
        if result.returncode or result.stdout.strip() != "airkctl " + version:
            raise RuntimeError("Signed nested airkctl did not report its version")
        report["stages"].append("signed_airkctl_version_passed")
        stage = "signed_payload_inventory"
        if package.inventory(runtime) != files:
            raise RuntimeError("Signed payload changed during post-sign probes")
        report["status"] = "passed"
    except Exception as error:
        report["status"] = "failed"
        report["failure_stage"] = stage
        report["failure_type"] = type(error).__name__
        save()
        print("Signed Mac runtime execution failed: " + json.dumps(report, sort_keys=True))
        raise
    save()
    print("Verified signed Mac runtime execution: " + json.dumps(report, sort_keys=True))
    return report


def package_standalone_installer(target, identity_sha1, keychain):
    """Reuse this exact short-lived keychain for the native-only DMG wrapper."""
    if any(name in os.environ for name in SECRET_NAMES):
        raise ValueError("Installer packaging requires signing secrets to be removed first")
    subprocess.run([sys.executable, str(ROOT / "scripts/package-standalone-macos.py"),
        "--target", target, "--runtime-dir", f"artifacts/runtime/{target}",
        "--signed-dir", f"artifacts/signed/{target}", "--identity", identity_sha1,
        "--team-id", TEAM, "--notary-profile", "airk-ci", "--keychain", str(keychain),
        "--output", f"artifacts/standalone/{target}"], cwd=ROOT, check=True)


def sign(target, paired=False, standalone=False):
    if standalone and not paired:
        raise ValueError("Standalone installer packaging requires a complete native runtime")
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
        identity_sha1 = imported_identity(keychain)
        private_run(["/usr/bin/xcrun", "notarytool", "store-credentials", "airk-ci",
                     "--apple-id", values["APPLE_ID"], "--team-id", TEAM, "--keychain", str(keychain)],
                    input=values["APPLE_APP_SPECIFIC_PASSWORD"] + "\n")
        values.clear()
        subprocess.run([sys.executable, str(ROOT / "scripts/sign-native.py"),
                        *(["--runtime-dir", f"artifacts/runtime/{target}"] if paired else []),
                        "--target", target, "--identity", identity_sha1, "--team-id", TEAM,
                        "--notarize", "--keychain-profile", "airk-ci", "--keychain", str(keychain),
                        "--output", f"artifacts/signed/{target}"], cwd=ROOT, check=True)
        verify_signed_output(ROOT / "artifacts/signed" / target, paired, identity_sha1)
        if standalone:
            package_standalone_installer(target, identity_sha1, keychain)
    finally:
        try:
            security("list-keychains", "-d", "user", "-s", *original_keychains)
        finally:
            cleanup(directory)
    # Both hosted signing callers share this gate. In particular, no model or
    # installation subprocess runs while the imported signing keychain exists.
    qualify_signed_runtime(ROOT / "artifacts/signed" / target, paired, target)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=("aarch64-apple-darwin", "x86_64-apple-darwin"))
    parser.add_argument("--cleanup", action="store_true")
    parser.add_argument("--pair", action="store_true")
    parser.add_argument("--standalone-installer", action="store_true",
                        default=os.environ.get("AIRK_STANDALONE_INSTALLER") == "true",
                        help="Package the native-only installer using this same isolated identity")
    args = parser.parse_args()
    if args.cleanup:
        cleanup(runner_directory())
    elif args.target:
        sign(args.target, args.pair, args.standalone_installer)
    else:
        parser.error("Supply --target or --cleanup")


if __name__ == "__main__":
    main()
