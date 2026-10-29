# SPDX-License-Identifier: Apache-2.0
"""Build and stage a host-native executable pair; never package/publish a VSIX."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import plistlib
import shutil
import subprocess
import tempfile
import time
import tomllib


ROOT = Path(__file__).resolve().parents[1]


def load_script(name):
    specification = importlib.util.spec_from_file_location(name.replace("-", "_"), ROOT / "scripts" / (name + ".py"))
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def stage_inference_runtime(staging, operating_system, architecture):
    provisioner = load_script("provision-onnx-runtime")
    target = operating_system + "-" + architecture
    directory = Path(os.environ.get("AIRK_BUNDLED_RUNTIME_DIR", ROOT / "target/local-onnx-runtime")) / target
    rows = provisioner.provision(directory, target)
    for row in rows:
        destination = staging / inference_asset_path(operating_system, row["name"])
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(directory / row["name"], destination)


def verify_embedded_models(staging, operating_system):
    report = json.loads(subprocess.check_output(
        [str(staging / runtime_names(operating_system)[1]), "--verify-bundled-models"],
        text=True, encoding="utf-8"))
    return load_script("int8-policy").verify_embedded(report)


def cargo_binary(name, profile, features):
    command = ["cargo", "build", "--locked", "-p", "airk-cli", "--bin", name,
               "--no-default-features", "--message-format=json-render-diagnostics"]
    if profile == "release":
        command.append("--release")
    if features:
        command += ["--features", features]
    result = subprocess.run(command, cwd=ROOT, text=True, encoding="utf-8",
                            stdout=subprocess.PIPE, check=True)
    executable = None
    for line in result.stdout.splitlines():
        event = json.loads(line)
        if event.get("reason") == "compiler-artifact" and event.get("target", {}).get("name") == name:
            if event.get("executable"):
                executable = Path(event["executable"])
    if not executable or not executable.is_file():
        raise RuntimeError(f"Cargo did not produce {name}")
    return executable


def check_lightweight_dependencies():
    result = subprocess.check_output([
        "cargo", "tree", "--locked", "-p", "airk-cli", "--no-default-features",
        "--edges", "normal", "--prefix", "none", "--format", "{p}"],
        cwd=ROOT, text=True, encoding="utf-8")
    dependencies = {line.split()[0] for line in result.splitlines() if line.strip()}
    forbidden = sorted(name for name in dependencies if name in ("tokenizers", "ort", "ort-sys") or name.startswith("tract-"))
    if forbidden:
        raise RuntimeError(f"Main executable includes inference dependencies: {forbidden}")


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def source_identity():
    names = subprocess.check_output([
        "git", "-c", f"safe.directory={ROOT.as_posix()}", "ls-files", "-z",
        "--cached", "--others", "--exclude-standard", "--", "Cargo.toml", "Cargo.lock",
        "rust-toolchain.toml", ".cargo", "crates", "scripts/build-service-runtime.py",
        "scripts/provision-onnx-runtime.py", "scripts/provision-local-model.py", "scripts/int8-policy.py",
        "scripts/macho-identity.py",
        "scripts/license-notices.py", "LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md", "third_party"], cwd=ROOT)
    result = hashlib.sha256()
    files = sorted(set(name for name in names.split(b"\0") if name))
    for name in files:
        path = ROOT / os.fsdecode(name)
        if path.is_symlink() or not path.resolve().is_relative_to(ROOT):
            raise RuntimeError("Source input escapes the runtime workspace")
        result.update(len(name).to_bytes(8, "big"))
        result.update(name)
        result.update(bytes.fromhex(digest(path)) if path.is_file() else b"missing")
    return {"sha256": result.hexdigest(), "files": len(files)}


def publish_staging(staging, destination):
    # A just-executed Windows image can briefly retain a scan/share handle. Retry
    # only this owned staging rename; persistent permission/policy failures stay errors.
    deadline = time.monotonic() + 5
    while True:
        try:
            os.rename(staging, destination)
            return
        except OSError as error:
            if os.name != "nt" or error.winerror not in (5, 32, 33) or time.monotonic() >= deadline:
                raise
            time.sleep(0.1)


def seal_install_payload(staging, version, operating_system, architecture):
    """Inventory the complete offline payload; its digest needs independent approval."""
    for name in ("LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md"):
        shutil.copyfile(ROOT / name, staging / name)
    subprocess.run([os.sys.executable, str(ROOT / "scripts" / "license-notices.py"),
                    "--output", str(staging / "notices")], cwd=ROOT, check=True)
    files = {item.relative_to(staging).as_posix(): digest(item)
             for item in sorted(staging.rglob("*")) if item.is_file()}
    if len(files) > 2048:
        raise RuntimeError("Offline runtime inventory exceeds installer file budget")
    manifest = {"schema": 1, "version": version, "os": operating_system,
                "arch": architecture, "files": files}
    (staging / "airk-install.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


def write_pair_metadata(staging, version, operating_system, architecture):
    main, helper = runtime_names(operating_system)
    manifest = {"schema": 1, "version": version, "os": operating_system, "arch": architecture,
                "airk_sha256": digest(staging / main), "embed_sha256": digest(staging / helper)}
    (staging / "airk-runtime.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    for executable in (main, helper):
        identity = subprocess.check_output([str(staging / executable), "--version"], text=True, encoding="utf-8").strip()
        if identity != f"{Path(executable).stem} {version}":
            raise RuntimeError(f"Unexpected executable identity: {identity}")
    return manifest


def runtime_names(operating_system):
    if operating_system == "macos":
        return ("AIRK.app/Contents/MacOS/airk", "AIRK.app/Contents/MacOS/airk-embed")
    return ("airk.exe", "airk-embed.exe") if operating_system == "windows" else ("airk", "airk-embed")


def signing_names(operating_system, version="1.6.0"):
    names = runtime_names(operating_system)
    return names + (("AIRK.app/Contents/MacOS/libonnxruntime.dylib",) if operating_system == "macos" and int8_version(version) else ())


def int8_version(version):
    return tuple(int(v) for v in version.split(".")[:2]) >= (1, 6)


def inference_asset_path(operating_system, name):
    # Contents/MacOS is a code directory: codesign treats even license text there
    # as nested code. Keep non-code resources outside it on both Mac targets.
    if operating_system == "macos" and name in ("ONNXRUNTIME-LICENSE", "ONNXRUNTIME-ThirdPartyNotices.txt"):
        parent = Path("AIRK.app/Contents/Resources/onnxruntime")
    else:
        parent = Path(runtime_names(operating_system)[1]).parent
    return (parent / name).as_posix()


def inference_assets(operating_system, architecture):
    return {inference_asset_path(operating_system, row["name"]): row
            for row in load_script("provision-onnx-runtime").entries(operating_system + "-" + architecture)}


def macos_bundle_files(version):
    """Static signed resources; mutable install/control paths never enter the bundle."""
    files = {"AIRK.app/Contents/Info.plist": plistlib.dumps({
        "CFBundleIdentifier": "io.iokaio.airk.service", "CFBundleName": "AIRK Service",
        "CFBundleExecutable": "airk", "CFBundlePackageType": "APPL",
        "CFBundleVersion": version, "CFBundleShortVersionString": version,
        "LSMinimumSystemVersion": "13.3" if int8_version(version) else "13.0", "LSBackgroundOnly": True,
    }, sort_keys=True)}
    for channel in ("stable", "preview"):
        label = "io.iokaio.airk.service." + channel
        files[f"AIRK.app/Contents/Library/LaunchAgents/{label}.plist"] = plistlib.dumps({
            "Label": label, "BundleProgram": "Contents/MacOS/airk",
            "ProgramArguments": ["airk", "runtime", "login-foreground", "--channel", channel],
            "RunAtLoad": True, "KeepAlive": {"SuccessfulExit": False},
            "ThrottleInterval": 30, "ProcessType": "Background",
        }, sort_keys=True)
    return files


def arrange_macos_bundle(staging, version):
    executables = runtime_names("macos")
    for source, destination in zip(("airk", "airk-embed"), executables):
        target = staging / destination
        target.parent.mkdir(parents=True, exist_ok=True)
        (staging / source).rename(target)
    for name, data in macos_bundle_files(version).items():
        target = staging / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)


def sign_development_macos_bundle(staging):
    # An ad-hoc signature enables native development fixtures. It is never a
    # Developer ID, notarization or independent distribution-approval claim.
    subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", "--timestamp=none",
                    str(staging / "AIRK.app/Contents/MacOS/libonnxruntime.dylib")], check=True)
    subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", "--options", "runtime",
                    "--timestamp=none", str(staging / runtime_names("macos")[1])], check=True)
    subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", "--options", "runtime",
                    "--timestamp=none", str(staging / "AIRK.app")], check=True)
    subprocess.run(["/usr/bin/codesign", "--verify", "--strict", str(staging / "AIRK.app")], check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--profile", choices=["debug", "release"], default="debug")
    args = parser.parse_args()
    if platform.system() == "Darwin":
        configured = os.environ.get("MACOSX_DEPLOYMENT_TARGET", "13.3")
        if configured != "13.3":
            raise RuntimeError("The INT8 Mac bundle requires MACOSX_DEPLOYMENT_TARGET=13.3; qualify another baseline separately")
        os.environ["MACOSX_DEPLOYMENT_TARGET"] = "13.3"
    destination = args.destination.absolute()
    if destination.exists() or destination.is_symlink():
        raise SystemExit("Destination already exists; select a fresh runtime directory")
    destination.parent.mkdir(parents=True, exist_ok=True)
    source = source_identity()
    load_script("int8-policy").catalog()
    check_lightweight_dependencies()
    with tempfile.TemporaryDirectory(prefix=".airk-runtime-stage-", dir=destination.parent) as temporary:
        staging = Path(temporary)
        # Copy helper before the separate feature-minimal main build. Cargo unifies
        # features within one invocation, so a workspace build cannot establish this boundary.
        helper = cargo_binary("airk-embed", args.profile, "embedding-worker")
        shutil.copy2(helper, staging / helper.name)
        main_binary = cargo_binary("airk", args.profile, None)
        shutil.copy2(main_binary, staging / main_binary.name)
        version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
        operating_system = {"Windows": "windows", "Darwin": "macos", "Linux": "linux"}.get(platform.system())
        architecture = {"amd64": "x86_64", "x86_64": "x86_64", "arm64": "aarch64", "aarch64": "aarch64"}.get(platform.machine().lower())
        if not operating_system or not architecture:
            raise RuntimeError("Unsupported host runtime pair target")
        if operating_system == "macos":
            arrange_macos_bundle(staging, version)
        stage_inference_runtime(staging, operating_system, architecture)
        verify_embedded_models(staging, operating_system)
        if operating_system == "macos":
            sign_development_macos_bundle(staging)
        manifest = write_pair_metadata(staging, version, operating_system, architecture)
        if source_identity() != source:
            raise RuntimeError("Native source changed during the paired build; staging was not published")
        receipt = {"schema": 1, "source": source, "profile": args.profile,
                   "runtime": manifest, "rustc": subprocess.check_output(
                       ["rustc", "--version", "--verbose"], text=True, encoding="utf-8").strip()}
        (staging / "airk-build.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
        seal_install_payload(staging, version, operating_system, architecture)
        if source_identity() != source:
            raise RuntimeError("Native source changed during offline payload staging")
        # The final directory is created only after both artifacts and identities verify.
        publish_staging(staging, destination)
    print(f"Staged {version} runtime pair: {destination}")


if __name__ == "__main__":
    main()
