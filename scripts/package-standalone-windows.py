# SPDX-License-Identifier: Apache-2.0
"""Package the signed native runtime as a per-user Inno Setup installer.

prepare emits an explicitly unsigned uninstaller for the existing Azure action;
build embeds its verified signed bytes; record publishes metadata only after the
outer EXE has also passed AIRK's existing Authenticode/publisher/EKU checks.
This script neither signs through a cloud service nor publishes a release.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
TARGET = "x86_64-pc-windows-msvc"
INNO_VERSION = "6.7.1"
SCRIPT = ROOT / "installers/windows/airk.iss"
spec = importlib.util.spec_from_file_location("standalone_windows_signer", ROOT / "scripts/sign-native.py")
signer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(signer)
package = signer.pair_package()


def plain(path, maximum=4_294_967_296):
    value = path.lstat()
    if (not stat.S_ISREG(value.st_mode) or value.st_nlink != 1
            or getattr(value, "st_file_attributes", 0) & 0x400 or value.st_size > maximum):
        raise ValueError("Linked, redirected or oversized installer input")
    return path


def save(path, value):
    if path.exists():
        raise ValueError("Existing installer receipt must not be overwritten")
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def native_inputs(runtime_dir, signed_dir):
    commit = signer.run(["git", "rev-parse", "HEAD"]).strip()
    lock = signer.digest(ROOT / "Cargo.lock")
    runtime, signing = signer.read_verified_pair(signed_dir, runtime_dir, TARGET, commit, lock)
    install = package.bounded_json(runtime / "airk-install.json")
    build = package.bounded_json(runtime / "airk-build.json", 65_536)
    if not package.runtime_builder().single_image_version(install["version"]):
        raise ValueError("Standalone installer requires the modern single-executable runtime")
    payload, _executables = package.validate_runtime(runtime, install["version"], TARGET, build["source"])
    if any(name.lower().endswith(".vsix") or "vscode" in name.lower() for name in payload):
        raise ValueError("Standalone installer must contain native CLI/MCP payload only")
    return runtime, {"version": install["version"], "target": TARGET, "commit": commit,
                     "cargo_lock_sha256": lock, "runtime_install_sha256": signer.digest(runtime / "airk-install.json"),
                     "runtime_source": build["source"], "payload": payload,
                     "native_signing_sha256": signer.digest(signed_dir / "signing.json")}


def compiler_identity(iscc):
    plain(iscc, 64 << 20)
    environment = dict(os.environ, AIRK_ISCC=str(iscc))
    result = subprocess.run(["pwsh", "-NoProfile", "-NonInteractive", "-Command",
                             "(Get-Item -LiteralPath $env:AIRK_ISCC).VersionInfo.ProductVersion"],
                            env=environment, check=True, capture_output=True, text=True, timeout=30)
    version = result.stdout.strip()
    if not re.fullmatch(re.escape(INNO_VERSION) + r"(?:\.\d+)?", version):
        raise ValueError("Expected Inno Setup " + INNO_VERSION + "; prepare the reviewed compiler explicitly")
    return {"version": version, "sha256": signer.digest(iscc)}


def compile_installer(context, work, *, first):
    definitions = {"AirkVersion": context["native"]["version"],
                   "AirkRuntime": context["runtime"], "AirkOutput": str(work / "compiled"),
                   "AirkArtifact": context["artifact"][:-4], "AirkUninstallers": str(work / "uninstallers"),
                   "AirkInstallSHA": context["native"]["runtime_install_sha256"],
                   "AirkBinarySHA": context["native"]["payload"]["airk.exe"],
                   "AirkInnoLicense": context["inno_license"]}
    if any(any(char in str(value) for char in ('\r', '\n', '"')) for value in definitions.values()):
        raise ValueError("Unsafe Inno Setup definition")
    command = [context["iscc"], "/Qp", "/NI", *[f"/D{name}={value}" for name, value in definitions.items()], str(SCRIPT)]
    result = subprocess.run(command, stdin=subprocess.DEVNULL, capture_output=True, text=True,
                            encoding="utf-8", errors="replace", timeout=600)
    (work / ("prepare.log" if first else "build.log")).write_text(result.stdout + result.stderr, encoding="utf-8")
    return result.returncode


def prepare(runtime_dir, signed_dir, output, iscc):
    if sys.platform != "win32":
        raise ValueError("Windows host required for installer compilation and trust verification")
    runtime, native = native_inputs(runtime_dir, signed_dir)
    compiler = compiler_identity(iscc)
    license_file = plain(iscc.parent / "license.txt", 65_536)
    output.mkdir(parents=True, exist_ok=False)
    work = output / "work"
    (work / "uninstallers").mkdir(parents=True)
    (work / "compiled").mkdir()
    artifact = f'airk-{native["version"]}-{TARGET}-installer.exe'
    context = {"schema": 1, "runtime": str(runtime.resolve()), "original": str(runtime_dir.resolve()),
               "signed": str(signed_dir.resolve()), "iscc": str(iscc.resolve()), "compiler": compiler,
               "inno_license": str(license_file.resolve()), "inno_license_sha256": signer.digest(license_file),
               "installer_source_sha256": signer.digest(SCRIPT), "native": native, "artifact": artifact}
    save(output / "inputs.json", context)
    result = compile_installer(context, work, first=True)
    # ISCC's documented manual-signing first pass stops after writing the exact
    # persistent uninstaller. A different compiler failure is not a candidate.
    candidates = list((work / "uninstallers").glob("*.e32")) + list((work / "uninstallers").glob("*.e64"))
    if result == 0 or len(candidates) != 1 or (work / "compiled" / artifact).exists():
        raise ValueError("Compiler did not stop at its single unsigned-uninstaller boundary; inspect prepare.log")
    candidate = plain(candidates[0], 32 << 20)
    with candidate.open("rb") as stream:
        if stream.read(2) != b"MZ":
            raise ValueError("Expected native PE uninstaller from Inno Setup")
    shutil.copyfile(candidate, output / "uninstaller.exe")
    save(output / "prepared.json", {"schema": 1, "phase": "unsigned_uninstaller",
                                    "cache_name": candidate.name, "unsigned_sha256": signer.digest(candidate),
                                    "inputs_sha256": signer.digest(output / "inputs.json")})
    print("Prepared unsigned uninstaller; Azure signing and final EXE verification remain required")


def load_context(output):
    context = package.bounded_json(output / "inputs.json", 512 << 10)
    prepared = package.bounded_json(output / "prepared.json", 4096)
    if (context.get("schema") != 1 or prepared.get("schema") != 1
            or prepared.get("phase") != "unsigned_uninstaller"
            or prepared.get("inputs_sha256") != signer.digest(output / "inputs.json")
            or Path(prepared.get("cache_name", "")).name != prepared.get("cache_name")
            or not prepared["cache_name"].endswith((".e32", ".e64"))):
        raise ValueError("Installer preparation ownership differs")
    if context["installer_source_sha256"] != signer.digest(SCRIPT) or context["compiler"] != compiler_identity(Path(context["iscc"])):
        raise ValueError("Installer source/compiler changed after preparation")
    if context["inno_license_sha256"] != signer.digest(plain(Path(context["inno_license"]), 65_536)):
        raise ValueError("Inno Setup license changed after preparation")
    runtime, native = native_inputs(Path(context["original"]), Path(context["signed"]))
    if native != context["native"] or str(runtime.resolve()) != context["runtime"]:
        raise ValueError("Signed runtime changed after installer preparation")
    return context, prepared


def verified_signature(binary):
    plain(binary)
    policy = signer.AZURE_SIGNING
    identity = json.loads(signer.run(["pwsh", "-NoProfile", "-NonInteractive", "-File",
        ROOT / "scripts/verify-authenticode.ps1", "-Binary", binary,
        "-Publisher", policy["publisher"], "-IdentityEku", policy["identity_eku"]]))
    receipt = {"platform": "Windows", "identity": identity["thumbprint"], "team_id": None,
               "azure": policy, "binary_sha256": signer.digest(binary)}
    receipt["verification"] = signer.verify(binary, receipt)
    return receipt


def build(output):
    context, prepared = load_context(output)
    signed = verified_signature(output / "uninstaller.exe")
    if signed["binary_sha256"] == prepared["unsigned_sha256"]:
        raise ValueError("Uninstaller has not acquired a signature")
    cache = output / "work/uninstallers" / prepared["cache_name"]
    plain(cache, 32 << 20)
    if signer.digest(cache) != prepared["unsigned_sha256"]:
        raise ValueError("Unsigned uninstaller cache changed before its signed replacement")
    shutil.copyfile(output / "uninstaller.exe", cache)
    if compile_installer(context, output / "work", first=False) != 0:
        raise ValueError("Final installer compilation failed; inspect build.log")
    artifact = plain(output / "work/compiled" / context["artifact"])
    if signer.digest(cache) != signed["binary_sha256"]:
        raise ValueError("Compiler changed the verified signed uninstaller")
    shutil.copyfile(artifact, output / context["artifact"])
    save(output / "built.json", {"schema": 1, "phase": "unsigned_installer",
        "inputs_sha256": signer.digest(output / "inputs.json"), "unsigned_sha256": signer.digest(artifact),
        "uninstaller": {"unsigned_sha256": prepared["unsigned_sha256"], **signed}})
    print("Prepared installer with verified signed uninstaller; outer EXE still requires Azure signing")


def record(output):
    context, prepared = load_context(output)
    built = package.bounded_json(output / "built.json", 65_536)
    if (built.get("schema") != 1 or built.get("phase") != "unsigned_installer"
            or built.get("inputs_sha256") != signer.digest(output / "inputs.json")):
        raise ValueError("Installer build ownership differs")
    uninstaller = verified_signature(output / "uninstaller.exe")
    if built["uninstaller"] != {"unsigned_sha256": prepared["unsigned_sha256"], **uninstaller}:
        raise ValueError("Signed uninstaller changed after final compilation")
    artifact = output / context["artifact"]
    signing = verified_signature(artifact)
    if signing["binary_sha256"] == built["unsigned_sha256"]:
        raise ValueError("Installer has not acquired its outer signature")
    publish = output / "publish"
    publish.mkdir(exist_ok=False)
    shutil.copyfile(artifact, publish / artifact.name)
    native = context["native"]
    receipt = {"schema": "airk-standalone-installer/1", **{name: native[name] for name in
        ("version", "target", "commit", "cargo_lock_sha256", "runtime_install_sha256", "runtime_source")},
        "artifact": artifact.name, "artifact_sha256": signer.digest(artifact), "installer_kind": "windows_inno",
        "signing": signing, "uninstaller": built["uninstaller"], "compiler": context["compiler"],
        "installer_source_sha256": context["installer_source_sha256"],
        "payload": {**{"runtime/" + name: value for name, value in native["payload"].items()},
                    "notices/inno-setup/LICENSE.txt": context["inno_license_sha256"]}}
    save(publish / "installer.json", receipt)
    (publish / "SHA256SUMS").write_text(
        f'{signer.digest(publish / artifact.name)}  {artifact.name}\n'
        f'{signer.digest(publish / "installer.json")}  installer.json\n', encoding="utf-8")
    print("Verified Windows installer and uninstaller using AIRK's existing Azure signing policy")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stage", choices=("prepare", "build", "record"))
    parser.add_argument("--runtime-dir", type=Path)
    parser.add_argument("--signed-dir", type=Path)
    parser.add_argument("--output", type=Path, default=ROOT / "artifacts/standalone" / TARGET)
    parser.add_argument("--iscc", type=Path, default=Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)")) / "Inno Setup 6/ISCC.exe")
    args = parser.parse_args()
    if args.stage == "prepare":
        if args.runtime_dir is None or args.signed_dir is None:
            parser.error("prepare requires --runtime-dir and --signed-dir")
        prepare(args.runtime_dir, args.signed_dir, args.output, args.iscc)
    elif args.stage == "build":
        build(args.output)
    else:
        record(args.output)


if __name__ == "__main__":
    main()
