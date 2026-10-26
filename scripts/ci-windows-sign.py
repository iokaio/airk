# SPDX-License-Identifier: Apache-2.0
"""Stage and attest the exact Windows helper signed by the Azure action."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sys

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("sign_native", ROOT / "scripts/sign-native.py")
signer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(signer)
TARGET = "x86_64-pc-windows-msvc"


def guard():
    if (sys.platform != "win32" or os.environ.get("GITHUB_ACTIONS") != "true"
            or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted"
            or os.environ.get("GITHUB_REF") != "refs/heads/main"
            or os.environ.get("GITHUB_EVENT_NAME") != "workflow_dispatch"
            or os.environ.get("GITHUB_WORKFLOW_REF") !=
            "iokaio/airk/.github/workflows/github-release.yml@refs/heads/main"):
        raise ValueError("Azure signing requires the manual AIRK main-branch hosted workflow")


def paths():
    return (ROOT / "target" / TARGET / "release/airk.exe", ROOT / "artifacts/signed" / TARGET)


def prepare(paired=False):
    guard()
    if paired:
        signer.prepare_pair(ROOT / "artifacts/runtime" / TARGET, ROOT / "artifacts/signed" / TARGET, TARGET)
        return add_signtool()
    source, output = paths()
    if source.is_symlink() or not source.is_file():
        raise ValueError("Verified source binary required")
    output.mkdir(parents=True, exist_ok=False)
    shutil.copyfile(source, output / "airk.exe")
    add_signtool()


def add_signtool():
    sdk = Path(os.environ["ProgramFiles(x86)"]) / "Windows Kits/10/bin"
    signtools = sorted(sdk.glob("*/x64/signtool.exe"))
    if not signtools:
        raise ValueError("Runner Windows SDK SignTool unavailable")
    with open(os.environ["GITHUB_PATH"], "a", encoding="utf-8") as path:
        path.write(str(signtools[-1].parent) + "\n")


def record(paired=False):
    guard()
    if paired:
        return record_pair()
    source, output = paths()
    binary = output / "airk.exe"
    if binary.is_symlink() or source.is_symlink() or (output / "signing.json").exists():
        raise ValueError("Linked binary or existing signing receipt rejected")
    policy = signer.AZURE_SIGNING
    identity = json.loads(signer.run([
        "pwsh", "-NoProfile", "-NonInteractive", "-File", ROOT / "scripts/verify-authenticode.ps1",
        "-Binary", binary, "-Publisher", policy["publisher"], "-IdentityEku", policy["identity_eku"]]))
    receipt = {"schema": "airk-native-signing/1", "platform": "Windows", "target": TARGET,
               "identity": identity["thumbprint"], "team_id": None, "azure": policy,
               "source_sha256": signer.digest(source), "binary_sha256": signer.digest(binary),
               "commit": signer.run(["git", "rev-parse", "HEAD"]).strip(),
               "cargo_lock_sha256": signer.digest(ROOT / "Cargo.lock"), "notarization": None}
    receipt["verification"] = signer.verify(binary, receipt)
    (output / "signing.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print("Verified Azure-signed Windows helper")


def record_pair():
    source = (ROOT / "artifacts/runtime" / TARGET).resolve(strict=True)
    output = (ROOT / "artifacts/signed" / TARGET).resolve(strict=True)
    if (output / "signing.json").exists():
        raise ValueError("Existing pair signing receipt rejected")
    policy = signer.AZURE_SIGNING
    commit = signer.run(["git", "rev-parse", "HEAD"]).strip()
    lock_digest = signer.digest(ROOT / "Cargo.lock")
    components = {}
    for name in ("airk.exe", "airk-embed.exe"):
        binary = output / "runtime" / name
        if binary.is_symlink() or (source / name).is_symlink():
            raise ValueError("Linked service component rejected")
        identity = json.loads(signer.run(["pwsh", "-NoProfile", "-NonInteractive", "-File", ROOT / "scripts/verify-authenticode.ps1",
                                          "-Binary", binary, "-Publisher", policy["publisher"], "-IdentityEku", policy["identity_eku"]]))
        components[name] = {"schema": "airk-native-signing/1", "platform": "Windows", "target": TARGET,
                            "identity": identity["thumbprint"], "team_id": None, "azure": policy,
                            "source_sha256": signer.digest(source / name), "binary_sha256": signer.digest(binary),
                            "commit": commit, "cargo_lock_sha256": lock_digest, "notarization": None}
    signer.finish_pair(source, output, TARGET, components, commit, lock_digest)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stage", choices=("prepare", "record"))
    parser.add_argument("--pair", action="store_true")
    args = parser.parse_args()
    prepare(args.pair) if args.stage == "prepare" else record(args.pair)
