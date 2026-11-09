# SPDX-License-Identifier: Apache-2.0
"""Build and seal a host-native runtime; never package/publish a VSIX."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import plistlib
import re
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


def verify_embedded_models(staging, operating_system, version="1.8.13"):
    executable = runtime_executables(operating_system, version)[0 if single_image_version(version) else 1]
    report = json.loads(subprocess.check_output(
        [str(staging / executable), "--verify-bundled-models"],
        text=True, encoding="utf-8"))
    return load_script("int8-policy").verify_embedded(report)


def cargo_binary(name, profile, features):
    command = ["cargo", "build", "--locked", "-p", "airk-cli", "--bin", name,
               "--no-default-features", "--message-format=json-render-diagnostics", "--timings"]
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


def verify_static_inputs():
    location = os.environ.get("ORT_LIB_LOCATION")
    if not location or not Path(location).is_absolute():
        raise RuntimeError("Modern builds require verified static ORT inputs and ORT_LIB_LOCATION")
    compiler = subprocess.check_output(["rustc", "--version", "--verbose"], text=True, encoding="utf-8")
    target = next((line.removeprefix("host: ") for line in compiler.splitlines() if line.startswith("host: ")), None)
    if target is None:
        raise RuntimeError("Cannot determine native static ORT build target")
    provisioner = load_script("provision-static-onnx-runtime")
    verified = provisioner.verify_build(Path(location).parent, target)
    for name, value in verified["environment"].items():
        if os.environ.get(name) != value:
            raise RuntimeError("Static ORT child environment differs from its verified build receipt: " + name)
    def canonical_digest(value, limit):
        encoded = json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False, allow_nan=False).encode("utf-8")
        if len(encoded) > limit:
            raise RuntimeError("Verified static ORT input identity exceeds its bound")
        return hashlib.sha256(encoded).hexdigest()
    closure = verified.get("static_link_closure")
    if not isinstance(closure, dict) or closure.get("schema") != 1:
        raise RuntimeError("Verified static ORT inputs lack the pinned consumer link closure")
    supplement = verified.get("static_link_supplement")
    supplement_sha256 = None
    if supplement is not None:
        if (not isinstance(supplement, dict) or supplement.get("path") != "link-supplement-receipt.json"
                or supplement.get("sha256") != digest(Path(location).parent / "link-supplement-receipt.json")):
            raise RuntimeError("Verified supplemental static ORT receipt identity differs")
        supplement_sha256 = supplement["sha256"]
    recovery = verified.get("static_link_recovery")
    recovery_sha256 = None
    if recovery is not None:
        if (not isinstance(recovery, dict) or recovery.get("path") != "link-supplement-recovery.json"
                or recovery.get("sha256") != digest(Path(location).parent / "link-supplement-recovery.json")):
            raise RuntimeError("Verified static ORT recovery receipt identity differs")
        recovery_sha256 = recovery["sha256"]
    return {"build_receipt_sha256": digest(Path(location).parent / "build-receipt.json"), "target": target,
            "static_link_closure_sha256": canonical_digest(closure, 65_536),
            "static_link_supplement_sha256": supplement_sha256,
            "static_link_recovery_sha256": recovery_sha256,
            "verified_inputs_sha256": canonical_digest(verified, 4 * 1024 * 1024)}


def packaged_static_notice_path(source):
    """Bound native payload depth without discarding the upstream legal path."""
    if (not isinstance(source, str) or len(source.encode("utf-8")) > 4096 or "\\" in source or ":" in source
            or any(ord(character) < 32 for character in source)
            or any(part in ("", ".", "..") for part in source.split("/"))):
        raise ValueError("Unsafe static ORT notice source path")
    parts = source.split("/")
    if (len(parts) < 3 or parts[0] != "notices"
            or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_-]{0,63}", parts[1])
            or not re.fullmatch(r"(?i)(?:LICENSE|COPYING|NOTICE)(?:[._-].*)?|ThirdPartyNotices\.txt", parts[-1])):
        raise ValueError("Unsafe static ORT legal notice identity")
    identity = hashlib.sha256(source.encode("utf-8")).hexdigest()
    return f"notices/static-onnx-runtime/{parts[1]}/LICENSE-{identity}.txt"


def stage_static_notices(staging, inputs):
    destination = Path(os.environ["ORT_LIB_LOCATION"]).parent
    receipt_file = destination / "build-receipt.json"
    if digest(receipt_file) != inputs["build_receipt_sha256"]:
        raise RuntimeError("Static ORT build receipt changed before notice staging")
    receipt = json.loads(receipt_file.read_text(encoding="utf-8"))
    rows = receipt["notices"]
    if not isinstance(rows, list) or not 1 <= len(rows) <= 2048:
        raise RuntimeError("Static ORT notice inventory exceeds its bound")
    prefix = "notices/static-onnx-runtime/"
    files, sources = {}, {}
    for row in rows:
        name = packaged_static_notice_path(row["path"])
        source = destination / row["path"]
        if source.is_symlink() or not source.is_file() or source.stat().st_size != row["bytes"] or source.stat().st_size > 2 * 1024 * 1024 or digest(source) != row["sha256"] or name in files:
            raise RuntimeError("Static ORT notice bytes differ from verified build")
        target = staging / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
        files[name] = row["sha256"]
        sources[name] = {"path":row["path"], "bytes":row["bytes"], "sha256":row["sha256"]}
    metadata = {"schema": 2, "ort_version": "1.22.1", "target": inputs["target"],
                "build_receipt_sha256": inputs["build_receipt_sha256"],
                "lock_sha256": digest(ROOT / "scripts/static-onnx-runtime-lock.json"), "files": files, "sources": sources}
    # Apply the same complete mapping/coverage contract before sealing as the
    # native/VSIX packaging paths use when verifying the resulting payload.
    static_notice_inventory(metadata, inputs["target"], files)
    (staging / prefix / "inventory.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")


def static_notice_inventory(metadata, target, files):
    prefix = "notices/static-onnx-runtime/"
    if (not isinstance(metadata, dict) or set(metadata) != {"schema", "ort_version", "target", "lock_sha256", "build_receipt_sha256", "files", "sources"}
            or metadata.get("schema") != 2 or metadata.get("ort_version") != "1.22.1" or metadata.get("target") != target
            or metadata.get("lock_sha256") != digest(ROOT / "scripts/static-onnx-runtime-lock.json")
            or not isinstance(metadata.get("build_receipt_sha256"), str) or not re.fullmatch(r"[0-9a-f]{64}", metadata["build_receipt_sha256"])
            or not isinstance(metadata.get("files"), dict) or not 1 <= len(metadata["files"]) <= 2048
            or not isinstance(metadata.get("sources"), dict) or set(metadata["sources"]) != set(metadata["files"])):
        raise ValueError("Static ORT notice identity or bound differs")
    lock = json.loads((ROOT / "scripts/static-onnx-runtime-lock.json").read_text(encoding="utf-8"))
    required = {"onnxruntime", *[row["name"] for row in lock["dependencies"]]}
    observed, source_paths = set(), set()
    for name, expected in metadata["files"].items():
        source = metadata["sources"][name]
        if (not isinstance(source, dict) or set(source) != {"path", "bytes", "sha256"}
                or type(source["bytes"]) is not int or not 0 <= source["bytes"] <= 2 * 1024 * 1024
                or source["sha256"] != expected or name != packaged_static_notice_path(source["path"])
                or not isinstance(expected, str) or not re.fullmatch(r"[0-9a-f]{64}", expected)
                or files.get(name) != expected or source["path"] in source_paths):
            raise ValueError("Static ORT notice content or source mapping differs")
        source_paths.add(source["path"])
        observed.add(source["path"].split("/")[1])
    if not required.issubset(observed) or not {"notices/onnxruntime/LICENSE", "notices/onnxruntime/ThirdPartyNotices.txt"}.issubset(source_paths):
        raise ValueError("Static ORT selected dependency notices are incomplete")
    return {prefix + "inventory.json", *metadata["files"]}


def windows_import_tool():
    available = shutil.which("dumpbin")
    if available:
        return available
    installer = Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)")) / "Microsoft Visual Studio/Installer/vswhere.exe"
    found = subprocess.check_output([str(installer), "-latest", "-products", "*", "-requires",
        "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-find",
        "VC/Tools/MSVC/*/bin/Hostx64/x64/dumpbin.exe"], text=True, encoding="utf-8", timeout=30)
    paths = [Path(line.strip()) for line in found.splitlines() if line.strip()]
    if len(paths) != 1 or not paths[0].is_absolute() or not paths[0].is_file():
        raise RuntimeError("Exactly one installed native MSVC import inspector is required")
    return str(paths[0])


def verify_static_imports(executable, operating_system):
    """Inspect the produced image's actual imports; missing tools fail closed."""
    command = {"windows": [windows_import_tool(), "/NOLOGO", "/DEPENDENTS"] if operating_system == "windows" else [],
               "macos": ["/usr/bin/otool", "-L"],
               "linux": ["readelf", "-d"]}[operating_system] + [str(executable)]
    completed = subprocess.run(command, check=True, text=True, encoding="utf-8",
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
    report = completed.stdout
    if not report.strip() or len(report.encode("utf-8")) > 8192:
        raise RuntimeError("Native import report is empty or exceeds its bound")
    if re.search(r"(?:lib)?onnxruntime[^\s/\\]*(?:\.dll|\.dylib|\.so)", report, re.IGNORECASE):
        raise RuntimeError("Single-image executable imports an external ONNX Runtime library")
    return {"schema": 1, "command": command, "stdout": report,
            "external_onnxruntime_imports": False}


def verify_model_smoke(staging, operating_system, version):
    executable = runtime_executables(operating_system, version)[0]
    report = json.loads(subprocess.check_output([str(staging / executable), "--verify-model-smoke"],
                                              text=True, encoding="utf-8", timeout=120))
    if (type(report.get("schema")) is not int or report["schema"] != 1 or report.get("source") != "embedded"
            or report.get("engine") != "onnxruntime-cpu/1.22.1" or report.get("dimensions") != 384
            or report.get("repeat_equal") is not True
            or report.get("profile") not in load_script("int8-policy").APPROVED
            or not isinstance(report.get("legacy_identity"), str) or not re.fullmatch(r"[0-9a-f]{64}", report["legacy_identity"])
            or not isinstance(report.get("pipeline_digest"), str) or not re.fullmatch(r"[0-9a-f]{64}", report["pipeline_digest"])):
        raise RuntimeError("Single-image embedded inference smoke contract differs")
    return report


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def source_identity():
    names = subprocess.check_output([
        "git", "-c", f"safe.directory={ROOT.as_posix()}", "ls-files", "-z",
        "--cached", "--others", "--exclude-standard", "--", "Cargo.toml", "Cargo.lock",
        "rust-toolchain.toml", ".cargo", "crates", "scripts/build-service-runtime.py",
        "scripts/provision-onnx-runtime.py", "scripts/provision-local-model.py", "scripts/int8-policy.py",
        "scripts/provision-static-onnx-runtime.py", "scripts/static-onnx-runtime-lock.json",
        "scripts/provision-static-onnx-tools.py", "scripts/static-onnx-tools-lock.json",
        "scripts/adopt-neo-static-onnx-runtime.py",
        "scripts/static-onnx-link-closure.py",
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
    # Every sealed payload is distributable: unresolved license dispositions block it.
    subprocess.run([os.sys.executable, str(ROOT / "scripts" / "license-notices.py"),
                    "--output", str(staging / "notices"), "--release"], cwd=ROOT, check=True)
    files = {item.relative_to(staging).as_posix(): digest(item)
             for item in sorted(staging.rglob("*")) if item.is_file()}
    if len(files) > 2048:
        raise RuntimeError("Offline runtime inventory exceeds installer file budget")
    manifest = {"schema": 1, "version": version, "os": operating_system,
                "arch": architecture, "files": files}
    (staging / "airk-install.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


def write_pair_metadata(staging, version, operating_system, architecture):
    executables = runtime_executables(operating_system, version)
    manifest = {"schema": 2 if single_image_version(version) else 1,
                "version": version, "os": operating_system, "arch": architecture,
                "airk_sha256": digest(staging / executables[0])}
    if not single_image_version(version):
        manifest["embed_sha256"] = digest(staging / executables[1])
    (staging / "airk-runtime.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    for executable in executables:
        identity = subprocess.check_output([str(staging / executable), "--version"], text=True, encoding="utf-8").strip()
        if identity != f"{Path(executable).stem} {version}":
            raise RuntimeError(f"Unexpected executable identity: {identity}")
    return manifest


def runtime_names(operating_system):
    """Historical schema-1 pair names; modern callers use runtime_executables."""
    if operating_system == "macos":
        return ("AIRK.app/Contents/MacOS/airk", "AIRK.app/Contents/MacOS/airk-embed")
    return ("airk.exe", "airk-embed.exe") if operating_system == "windows" else ("airk", "airk-embed")


def single_image_version(version):
    return tuple(int(value) for value in version.split(".")) >= (1, 8, 14)


def runtime_executables(operating_system, version):
    names = runtime_names(operating_system)
    return names[:1] if single_image_version(version) else names


def runtime_metadata_matches(metadata, version, operating_system, architecture, files):
    """Strict versioned decoder: absence of a legacy helper is never inferred."""
    names = runtime_executables(operating_system, version)
    expected = {"schema": 2 if single_image_version(version) else 1,
                "version": version, "os": operating_system, "arch": architecture,
                "airk_sha256": files.get(names[0])}
    if not single_image_version(version):
        expected["embed_sha256"] = files.get(names[1])
    return all(files.get(name) for name in names) and metadata == expected


def signing_names(operating_system, version="1.6.0"):
    names = runtime_executables(operating_system, version)
    if operating_system == "macos":
        if int8_version(version) and not single_image_version(version):
            names += ("AIRK.app/Contents/MacOS/libonnxruntime.dylib",)
        if standalone_launcher_version(version):
            names += (launcher_name(operating_system, version),)
    return names


def standalone_launcher_version(version):
    return (1, 8, 8) <= tuple(int(v) for v in version.split(".")) < (1, 8, 14)


def launcher_name(operating_system, version):
    if operating_system == "macos" and standalone_launcher_version(version):
        return "AIRK.app/Contents/MacOS/airk-launcher-image"
    return runtime_names(operating_system)[0]


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


def inference_assets(operating_system, architecture, version="1.8.13"):
    if single_image_version(version):
        return {}
    return {inference_asset_path(operating_system, row["name"]): row
            for row in load_script("provision-onnx-runtime").entries(operating_system + "-" + architecture)}


def macos_bundle_files(version):
    """Static signed resources; mutable install/control paths never enter the bundle."""
    entry = "login-serve" if tuple(map(int, version.split("."))) >= (1, 8, 14) else "login-foreground"
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
            "ProgramArguments": ["airk", "runtime", entry, "--channel", channel],
            "RunAtLoad": True, "KeepAlive": {"SuccessfulExit": False},
            "ThrottleInterval": 30, "ProcessType": "Background",
        }, sort_keys=True)
    return files


def arrange_macos_bundle(staging, version):
    executables = runtime_executables("macos", version)
    for source, destination in zip(("airk", "airk-embed"), executables):
        target = staging / destination
        target.parent.mkdir(parents=True, exist_ok=True)
        (staging / source).rename(target)
    if standalone_launcher_version(version):
        # Outer bundle signing binds CFBundleExecutable to its resource seal.
        # Keep a distinct nested standalone image, signed before sealing the app;
        # only this separately inventoried file is copied into the stable launcher.
        image = staging / launcher_name("macos", version)
        shutil.copyfile(staging / executables[0], image)
        image.chmod(0o755)
    for name, data in macos_bundle_files(version).items():
        target = staging / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)


def sign_development_macos_bundle(staging):
    # An ad-hoc signature enables native development fixtures. It is never a
    # Developer ID, notarization or independent distribution-approval claim.
    # Ad-hoc code has no Team ID for hardened-runtime library validation.
    # Clear runtime flags on these development copies; sign-native.py applies
    # Developer ID and hardened runtime to every release signing component.
    version = plistlib.loads((staging / "AIRK.app/Contents/Info.plist").read_bytes())["CFBundleVersion"]
    for name in ("AIRK.app/Contents/MacOS/libonnxruntime.dylib", runtime_names("macos")[1]):
        if not single_image_version(version):
            subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", "--options", "0",
                            "--timestamp=none", str(staging / name)], check=True)
    image = staging / "AIRK.app/Contents/MacOS/airk-launcher-image"
    if image.exists():
        subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", "--options", "0",
                        "--timestamp=none", str(image)], check=True)
    subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", "--options", "0",
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
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    if not single_image_version(version):
        # This tree no longer contains the airk-embed helper target. Historical
        # pair payloads are built from their tagged sources and build scripts.
        raise SystemExit(f"Version {version} needs a historical runtime pair; build it from its tagged source")
    source = source_identity()
    load_script("int8-policy").catalog()
    static_inputs = verify_static_inputs()
    with tempfile.TemporaryDirectory(prefix=".airk-runtime-stage-", dir=destination.parent) as temporary:
        staging = Path(temporary)
        main_binary = cargo_binary("airk", args.profile, "local-inference")
        shutil.copy2(main_binary, staging / main_binary.name)
        operating_system = {"Windows": "windows", "Darwin": "macos", "Linux": "linux"}.get(platform.system())
        architecture = {"amd64": "x86_64", "x86_64": "x86_64", "arm64": "aarch64", "aarch64": "aarch64"}.get(platform.machine().lower())
        if not operating_system or not architecture:
            raise RuntimeError("Unsupported host runtime pair target")
        if operating_system == "macos":
            arrange_macos_bundle(staging, version)
        verify_embedded_models(staging, operating_system, version)
        if source_identity() != source:
            raise RuntimeError("Native source changed during the runtime build; staging was not published")
        if operating_system == "macos":
            sign_development_macos_bundle(staging)
        imports = verify_static_imports(staging / runtime_executables(operating_system, version)[0], operating_system)
        model_smoke = verify_model_smoke(staging, operating_system, version)
        if verify_static_inputs() != static_inputs:
            raise RuntimeError("Static ORT compiled inputs changed during the build")
        stage_static_notices(staging, static_inputs)
        manifest = write_pair_metadata(staging, version, operating_system, architecture)
        if source_identity() != source:
            raise RuntimeError("Native source changed during the paired build; staging was not published")
        receipt = {"schema": 1, "source": source, "profile": args.profile,
                   "runtime": manifest, "rustc": subprocess.check_output(
                       ["rustc", "--version", "--verbose"], text=True, encoding="utf-8").strip(),
                   "static_ort": static_inputs, "static_imports": imports, "embedded_model_smoke": model_smoke}
        receipt_text = json.dumps(receipt, indent=2) + "\n"
        if len(receipt_text.encode("utf-8")) > 16_384:
            raise RuntimeError("Complete build receipt exceeds the native metadata bound")
        (staging / "airk-build.json").write_text(receipt_text, encoding="utf-8")
        seal_install_payload(staging, version, operating_system, architecture)
        if source_identity() != source:
            raise RuntimeError("Native source changed during offline payload staging")
        # The final directory is created only after both artifacts and identities verify.
        publish_staging(staging, destination)
    print(f"Staged {version} {args.profile} runtime: {destination}")


if __name__ == "__main__":
    main()
