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
AZURE_SIGNING = {
    "endpoint": "https://wus2.codesigning.azure.net",
    "account": "ioka-llc-signing",
    "profile": "WritingForgePro",
    "publisher": "CN=Ioka LLC, O=Ioka LLC, L=Mapleton, S=Utah, C=US",
    "identity_eku": "1.3.6.1.4.1.311.97.88309284.513035131.587831003.613935669",
}


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


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
        azure = receipt.get("azure")
        if azure is not None and azure != AZURE_SIGNING:
            raise ValueError("Unexpected Azure signing policy")
        policy = (["-Publisher", azure["publisher"], "-IdentityEku", azure["identity_eku"]]
                  if azure is not None else [])
        identity = json.loads(run(["pwsh", "-NoProfile", "-NonInteractive", "-File",
                                   ROOT / "scripts/verify-authenticode.ps1", "-Binary", binary,
                                   "-Thumbprint", receipt["identity"], *policy]))
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


def read_verified_pair(directory, source, target, commit, lock_digest, signtool="signtool"):
    package = pair_package()
    receipt_path = directory / "signing.json"
    receipt = package.bounded_json(receipt_path, 65_536)
    if (receipt.get("schema") != "airk-service-pair-signing/1" or receipt.get("target") != target
            or receipt.get("commit") != commit or receipt.get("cargo_lock_sha256") != lock_digest
            or receipt.get("source_install_sha256") != digest(source / "airk-install.json")):
        raise ValueError("Pair signing receipt differs from tested source")
    runtime = directory / "runtime"
    original = package.bounded_json(source / "airk-install.json")
    signed = package.bounded_json(runtime / "airk-install.json")
    main, helper = package.runtime_builder().runtime_names("windows" if "windows" in target else "macos")
    signing_names = package.runtime_builder().signing_names("windows" if "windows" in target else "macos", original.get("version", "1.6.0"))
    components = receipt.get("components", {})
    ticket = {"AIRK.app/Contents/CodeResources"} if "apple" in target else set()
    if set(components) != set(signing_names) or set(original["files"]) - ticket != set(signed["files"]) - ticket:
        raise ValueError("Pair signing component inventory differs")
    mutable = {*signing_names, "airk-runtime.json", "airk-build.json"}
    if "apple" in target:
        mutable.add("AIRK.app/Contents/_CodeSignature/CodeResources")
        mutable.add("AIRK.app/Contents/CodeResources")
        if "AIRK.app/Contents/CodeResources" in signed["files"] and "AIRK.app/Contents/CodeResources" not in original["files"] and not stapled_app_ticket(components):
            raise ValueError("New app ticket lacks matching stapled notarization receipts")
    for name, expected in original["files"].items():
        if name not in mutable and signed["files"].get(name) != expected:
            raise ValueError("Signing changed unrelated runtime payload")
    for name in signing_names:
        component = components[name]
        if (component.get("schema") != "airk-native-signing/1" or component.get("target") != target
                or component.get("commit") != commit or component.get("cargo_lock_sha256") != lock_digest
                or component.get("source_sha256") != original["files"][name]
                or component.get("binary_sha256") != signed["files"][name]):
            raise ValueError("Signed component differs from tested executable")
        component["verification"] = verify(runtime / name, component, signtool)
    if "apple" in target:
        verify_app_bundle(runtime, components[main]["team_id"])
        if stapled_app_ticket(components):
            run(["/usr/bin/xcrun", "stapler", "validate", runtime / "AIRK.app"])
    if receipt.get("install_manifest_sha256") != digest(runtime / "airk-install.json"):
        raise ValueError("Signed pair installer digest differs")
    return runtime, receipt


def pair_package():
    import importlib.util
    specification = importlib.util.spec_from_file_location("airk_native_package", ROOT / "scripts/package-native.py")
    package = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(package)
    return package


def verify_app_bundle(runtime, team):
    if not re.fullmatch(r"[A-Z0-9]{10}", team):
        raise ValueError("Expected exact Apple bundle team identifier")
    run(["/usr/bin/codesign", "--verify", "--strict", "--verbose=2", "-R",
         f'=anchor apple generic and certificate leaf[subject.OU] = "{team}" and certificate leaf[field.1.2.840.113635.100.6.1.13] exists', runtime / "AIRK.app"])


def stapled_app_ticket(components):
    receipts = [component.get("notarization") for component in components.values()]
    return (len(receipts) in (2, 3) and all(isinstance(value, dict) and value.get("status") == "Accepted"
            and value.get("stapled") is True and value.get("scope") == "AIRK.app" and isinstance(value.get("id"), str) and value["id"] for value in receipts)
            and len({value["id"] for value in receipts}) == 1)


def prepare_pair(source, output, target):
    package = pair_package()
    source = source.resolve(strict=True)
    original_build = json.loads((source / "airk-build.json").read_text(encoding="utf-8"))
    original_install = json.loads((source / "airk-install.json").read_text(encoding="utf-8"))
    files, executables = package.validate_runtime(source, original_install["version"], target, original_build["source"])
    output = output.resolve()
    if not output.is_relative_to((ROOT / "artifacts").resolve()) or output == (ROOT / "artifacts").resolve():
        raise ValueError("Signing output must be a fresh directory inside artifacts/")
    output.mkdir(parents=True, exist_ok=False)
    runtime = output / "runtime"
    runtime.mkdir()
    for name in files:
        destination = runtime / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source / name, destination)
    if package.inventory(runtime) != files:
        raise ValueError("Runtime source changed while staging signing copies")
    (output / "pair-signing.pending.json").write_text(json.dumps({"schema": 1, "source": str(source), "target": target,
        "source_install_sha256": files["airk-install.json"]}, indent=2) + "\n", encoding="utf-8")
    return source, runtime, executables, files


def finish_pair(source, output, target, components, commit, lock_digest, signtool="signtool"):
    package = pair_package()
    runtime = output / "runtime"
    original_build = json.loads((source / "airk-build.json").read_text(encoding="utf-8"))
    original_install = json.loads((source / "airk-install.json").read_text(encoding="utf-8"))
    files, executables = package.validate_runtime(source, original_install["version"], target, original_build["source"])
    pending = json.loads((output / "pair-signing.pending.json").read_text(encoding="utf-8"))
    if pending != {"schema": 1, "source": str(source), "target": target, "source_install_sha256": files["airk-install.json"]}:
        raise ValueError("Signing source differs from prepared pair")
    staged = package.inventory(runtime)
    signing_names = package.runtime_builder().signing_names("windows" if "windows" in target else "macos", original_install["version"])
    mutable = set(signing_names)
    if "apple" in target:
        mutable.add("AIRK.app/Contents/_CodeSignature/CodeResources")
    ticket = {"AIRK.app/Contents/CodeResources"} if "apple" in target else set()
    if set(staged) - set(files) and not stapled_app_ticket(components):
        raise ValueError("New app ticket lacks matching stapled notarization receipts")
    if set(staged) - ticket != set(files) - ticket or any(staged[name] != files[name] for name in files if name not in mutable and name not in ticket):
        raise ValueError("Signing changed unrelated runtime payload")
    if set(components) != set(signing_names):
        raise ValueError("Both signed service components are required")
    for name, component in components.items():
        if (component.get("schema") != "airk-native-signing/1" or component.get("target") != target
                or component.get("commit") != commit or component.get("cargo_lock_sha256") != lock_digest
                or component["source_sha256"] != files[name] or component["binary_sha256"] != digest(runtime / name)):
            raise ValueError("Signed service component differs from tested build")
        component["verification"] = verify(runtime / name, component, signtool)
    if "apple" in target:
        verify_app_bundle(runtime, components[executables[0]]["team_id"])
        if stapled_app_ticket(components):
            run(["/usr/bin/xcrun", "stapler", "validate", runtime / "AIRK.app"])
    pair = json.loads((runtime / "airk-runtime.json").read_text(encoding="utf-8"))
    pair.update(airk_sha256=digest(runtime / executables[0]), embed_sha256=digest(runtime / executables[1]))
    (runtime / "airk-runtime.json").write_text(json.dumps(pair, indent=2) + "\n", encoding="utf-8")
    original_build["runtime"] = pair
    (runtime / "airk-build.json").write_text(json.dumps(original_build, indent=2) + "\n", encoding="utf-8")
    original_install["files"] = {name: value for name, value in package.inventory(runtime).items() if name != "airk-install.json"}
    (runtime / "airk-install.json").write_text(json.dumps(original_install, indent=2) + "\n", encoding="utf-8")
    receipt = {"schema": "airk-service-pair-signing/1", "target": target, "commit": commit,
               "cargo_lock_sha256": lock_digest, "source_install_sha256": digest(source / "airk-install.json"),
               "install_manifest_sha256": digest(runtime / "airk-install.json"), "components": components}
    package.validate_runtime(runtime, original_install["version"], target, original_build["source"])
    if package.inventory(source) != files:
        raise ValueError("Tested source changed during signing")
    (output / "signing.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(f"Verified signed native service pair: {runtime}")


def sign_pair(args):
    """Sign both staged copies, then seal their exact new installer identities."""
    output = args.output.resolve()
    source, runtime, executables, _ = prepare_pair(args.runtime_dir, output, args.target)
    if "apple" in args.target and (runtime / "AIRK.app/Contents/CodeResources").exists() and not args.notarize:
        raise ValueError("Re-signing a stapled app requires explicit --notarize to replace its ticket")
    commit = run(["git", "rev-parse", "HEAD"]).strip()
    lock_digest = digest(ROOT / "Cargo.lock")
    components = {}
    version = json.loads((runtime / "airk-install.json").read_text(encoding="utf-8"))["version"]
    for name in pair_package().runtime_builder().signing_names("macos" if "apple" in args.target else "windows", version):
        component_output = output / (Path(name).name + ".signature")
        command = [__import__("sys").executable, str(ROOT / "scripts/sign-native.py"), "--target", args.target,
                   "--source-file", str(source / name), "--output", str(component_output), "--identity", args.identity,
                   "--signtool", args.signtool]
        for flag in ("timestamp_url", "team_id", "keychain_profile", "keychain"):
            value = getattr(args, flag)
            if value:
                command += ["--" + flag.replace("_", "-"), str(value)]
        if args.notarize and "apple" not in args.target:
            command.append("--notarize")
        run(command, timeout=1500)
        binary, receipt = read_verified(component_output, source / name, args.target, commit, lock_digest, args.signtool)
        shutil.copyfile(binary, runtime / name)
        (runtime / name).chmod(0o755)
        components[name] = receipt
    if "apple" in args.target:
        keychain = ["--keychain", str(args.keychain)] if args.keychain else []
        # Seal the enclosing app after its two nested executables. Hashes remain
        # outside the signed bundle, avoiding a signature/resource digest cycle.
        run(["/usr/bin/codesign", "--force", "--sign", args.identity, "--options", "runtime", "--timestamp", *keychain, runtime / "AIRK.app"])
        components[executables[0]]["binary_sha256"] = digest(runtime / executables[0])
        verify_app_bundle(runtime, args.team_id)
        if args.notarize:
            with tempfile.TemporaryDirectory(prefix="airk-app-notary-") as temporary:
                archive = Path(temporary) / "AIRK.zip"
                with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED) as zipped:
                    for file in sorted((runtime / "AIRK.app").rglob("*")):
                        if file.is_file():
                            zipped.write(file, file.relative_to(runtime).as_posix())
                result = json.loads(run(["/usr/bin/xcrun", "notarytool", "submit", archive, "--keychain-profile", args.keychain_profile,
                                         *keychain, "--wait", "--timeout", "20m", "--output-format", "json"], timeout=1250))
                if result.get("status") != "Accepted" or not result.get("id"):
                    raise RuntimeError("Apple app notarization did not return Accepted")
                run(["/usr/bin/xcrun", "stapler", "staple", runtime / "AIRK.app"])
                run(["/usr/bin/xcrun", "stapler", "validate", runtime / "AIRK.app"])
                for receipt in components.values():
                    receipt["notarization"] = {"id": result["id"], "status": "Accepted", "stapled": True, "scope": "AIRK.app"}
    finish_pair(source, output, args.target, components, commit, lock_digest, args.signtool)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True, choices=("x86_64-pc-windows-msvc", "x86_64-apple-darwin", "aarch64-apple-darwin"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, help="Sign both executables from one tested native pair")
    parser.add_argument("--source-file", type=Path, help=argparse.SUPPRESS)
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
    if args.runtime_dir:
        if args.source_file:
            parser.error("Pair and single-component inputs are mutually exclusive")
        return sign_pair(args)
    source = args.source_file or ROOT / "target" / args.target / "release" / ("airk.exe" if windows else "airk")
    if args.source_file:
        source = source.resolve(strict=True)
        expected = ("airk.exe", "airk-embed.exe") if windows else ("airk", "airk-embed", "libonnxruntime.dylib")
        owner = source.parent
        if not windows and source.parent.name == "MacOS":
            owner = source.parents[3]
        if source.name not in expected or not source.is_relative_to((ROOT / "artifacts").resolve()) or not (owner / "airk-install.json").is_file():
            parser.error("Signing component must belong to a staged artifact runtime pair")
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
        # Apple Silicon linkers emit an ad-hoc signature. Replace it only on
        # this fresh copy, then require the Developer ID signature below.
        run(["/usr/bin/codesign","--force","--sign",args.identity,"--options","runtime","--timestamp",*keychain,binary])
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
