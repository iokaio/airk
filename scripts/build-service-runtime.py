# SPDX-License-Identifier: Apache-2.0
"""Build and seal a host-native runtime; never package/publish a VSIX."""
import argparse
import functools
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


@functools.cache
def split_receipts():
    """The C7 owner, loaded beside this file (not through ROOT, which tests may relocate)."""
    specification = importlib.util.spec_from_file_location("airk_split_receipts", Path(__file__).with_name("split_receipts.py"))
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
    executable = (service_executable(operating_system, version) if single_image_version(version)
                  else runtime_executables(operating_system, version)[1])
    report = json.loads(subprocess.check_output(
        [str(staging / executable), "--verify-bundled-models"],
        text=True, encoding="utf-8"))
    return load_script("int8-policy").verify_embedded(report)


def profile_arguments(profile):
    """Cargo's profile selection: `debug`, `release` (CI and published builds) or `release-local`."""
    if profile == "release":
        return ["--release"]
    if profile == "release-local":
        return ["--profile", "release-local"]
    if profile == "debug":
        return []
    raise ValueError(f"unknown build profile {profile!r}")


def cargo_binary(name, profile, features, package="airk-cli"):
    """The one Cargo build owner: `airk` from airk-cli (shipping features), `airkctl` from its crate."""
    command = ["cargo", "build", "--locked", "-p", package, "--bin", name,
               "--no-default-features", "--message-format=json-render-diagnostics", "--timings"]
    command += profile_arguments(profile)
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


PAIR_FEATURES = "airk-cli/local-inference"


def cargo_pair(profile):
    """Local installer builds: `airk` (shipping features) and `airkctl` in one Cargo invocation, so
    the crates both share compile once. Feature unification can change the features of crates already
    in `airkctl`'s graph, never add a crate; `check_lightweight_dependencies(unified=True)` proves the
    closure of this exact resolution."""
    command = ["cargo", "build", "--locked", "-p", "airk-cli", "--bin", "airk", "-p", "airkctl",
               "--bin", "airkctl", "--no-default-features", "--features", PAIR_FEATURES,
               "--message-format=json-render-diagnostics", "--timings", *profile_arguments(profile)]
    result = subprocess.run(command, cwd=ROOT, text=True, encoding="utf-8",
                            stdout=subprocess.PIPE, check=True)
    found = {}
    for line in result.stdout.splitlines():
        event = json.loads(line)
        name = event.get("target", {}).get("name")
        if event.get("reason") == "compiler-artifact" and name in ("airk", "airkctl") and event.get("executable"):
            found[name] = Path(event["executable"])
    for name in ("airk", "airkctl"):
        if name not in found or not found[name].is_file():
            raise RuntimeError(f"Cargo did not produce {name}")
    return found["airk"], found["airkctl"]


def airkctl_closure(tree_output):
    """Package names under the `airkctl` root of `cargo tree --prefix depth` output."""
    names, inside = set(), False
    for line in tree_output.splitlines():
        line = line.strip()
        if not line:
            continue
        digits = len(line) - len(line.lstrip("0123456789"))
        if digits == 0:
            continue
        depth, package = int(line[:digits]), line[digits:].split()[0]
        if depth == 0:
            inside = package == "airkctl"
        if inside:
            names.add(package)
    return names


def check_lightweight_dependencies(unified=False):
    """The CLI image's normal dependency closure has no inference crate (split plan §10.5.2).
    `unified` checks the closure under the paired build's feature resolution (`cargo_pair`)."""
    roots = ["-p", "airkctl"] + (["-p", "airk-cli", "--features", PAIR_FEATURES] if unified else [])
    result = subprocess.check_output([
        "cargo", "tree", "--locked", *roots, "--no-default-features",
        "--edges", "normal", "--prefix", "depth", "--format", "{p}"],
        cwd=ROOT, text=True, encoding="utf-8")
    dependencies = airkctl_closure(result)
    if "airkctl" not in dependencies:
        raise RuntimeError("Cargo tree did not report the airkctl package")
    forbidden = sorted(name for name in dependencies
                       if name in ("tokenizers", "ort") or name.startswith(("ort-", "tract-")))
    if forbidden:
        raise RuntimeError(f"airkctl includes inference dependencies: {forbidden}")


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
    aggregate = verified.get("static_aggregate")
    if aggregate is not None:
        if (closure is not None or verified.get("static_link_supplement") is not None
                or verified.get("static_link_recovery") is not None):
            raise RuntimeError("Verified static ORT inputs mix aggregate and legacy consumer identities")
        receipt_file = Path(location).parent / "static-aggregate-receipt.json"
        if (not isinstance(aggregate, dict) or set(aggregate) != {"path", "sha256", "receipt"}
                or aggregate.get("path") != receipt_file.name
                or receipt_file.is_symlink() or not receipt_file.is_file()
                or receipt_file.stat().st_size > 65_536
                or aggregate.get("sha256") != digest(receipt_file)
                or canonical_digest(aggregate.get("receipt"), 65_536)
                != canonical_digest(json.loads(receipt_file.read_bytes()), 65_536)):
            raise RuntimeError("Verified static ORT aggregate receipt identity differs")
        # The owner already verified the genuine archive and original source
        # closure. Bind that exact descriptor, never a fictional split layout.
        closure = aggregate
    elif not isinstance(closure, dict) or closure.get("schema") != 1:
        raise RuntimeError("Verified static ORT inputs lack a verified consumer link identity")
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
    owner = load_script('provision-static-onnx-runtime')
    owner.validate_lock()
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
    metadata = {"schema": 2, "ort_version": owner.VERSION, "target": inputs["target"],
                "build_receipt_sha256": inputs["build_receipt_sha256"],
                "lock_sha256": digest(ROOT / "scripts/static-onnx-runtime-lock.json"), "files": files, "sources": sources}
    # Apply the same complete mapping/coverage contract before sealing as the
    # native/VSIX packaging paths use when verifying the resulting payload.
    static_notice_inventory(metadata, inputs["target"], files)
    (staging / prefix / "inventory.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")


def static_notice_inventory(metadata, target, files):
    prefix = "notices/static-onnx-runtime/"
    owner = load_script('provision-static-onnx-runtime')
    owner.validate_lock()
    if (not isinstance(metadata, dict) or set(metadata) != {"schema", "ort_version", "target", "lock_sha256", "build_receipt_sha256", "files", "sources"}
            or metadata.get("schema") != 2 or metadata.get("ort_version") != owner.VERSION or metadata.get("target") != target
            or metadata.get("lock_sha256") != digest(ROOT / "scripts/static-onnx-runtime-lock.json")
            or not isinstance(metadata.get("build_receipt_sha256"), str) or not re.fullmatch(r"[0-9a-f]{64}", metadata["build_receipt_sha256"])
            or not isinstance(metadata.get("files"), dict) or not 1 <= len(metadata["files"]) <= 2048
            or not isinstance(metadata.get("sources"), dict) or set(metadata["sources"]) != set(metadata["files"])):
        raise ValueError("Static ORT notice identity or bound differs")
    lock = owner.LOCK
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


# Byte markers that only an image linking inference code contains: the ONNX Runtime C API
# entry symbol and the registry source paths Rust embeds in panic locations of the
# inference crates. airkctl must contain none of them.
INFERENCE_MARKERS = (b"OrtGetApiBase", b"onnxruntime_c_api", b"tokenizers-0.", b"ort-sys-2.", b"tract-onnx")
MAX_CLI_IMAGE_BYTES = 256 << 20


def scan_inference_markers(path):
    """Bounded, chunked scan of the CLI image for INFERENCE_MARKERS; oversize fails closed."""
    size = path.stat().st_size
    if not 0 < size <= MAX_CLI_IMAGE_BYTES:
        raise RuntimeError("CLI image is empty or exceeds its inspection bound")
    overlap = max(len(marker) for marker in INFERENCE_MARKERS) - 1
    found, tail = set(), b""
    with path.open("rb") as stream:
        while block := stream.read(1 << 20):
            window = tail + block
            found.update(marker for marker in INFERENCE_MARKERS if marker in window)
            tail = window[-overlap:]
    return sorted(marker.decode() for marker in found)


def verify_cli_imports(executable, operating_system):
    """airkctl: no external ONNX Runtime import and no linked inference code."""
    report = verify_static_imports(executable, operating_system)
    markers = scan_inference_markers(Path(executable))
    if markers:
        raise RuntimeError(f"airkctl image contains inference code markers: {markers}")
    return {**report, "inference_symbols": False}


def verify_model_smoke(staging, operating_system, version):
    static_owner = load_script("provision-static-onnx-runtime")
    static_owner.validate_lock()
    expected_engine = "onnxruntime-cpu/" + static_owner.VERSION
    executable = service_executable(operating_system, version)
    report = json.loads(subprocess.check_output([str(staging / executable), "--verify-model-smoke"],
                                              text=True, encoding="utf-8", timeout=120))
    if (type(report.get("schema")) is not int or report["schema"] != 1 or report.get("source") != "embedded"
            or report.get("engine") != expected_engine or report.get("dimensions") != 384
            or report.get("repeat_equal") is not True
            or report.get("profile") not in load_script("int8-policy").APPROVED
            or not isinstance(report.get("legacy_identity"), str) or not re.fullmatch(r"[0-9a-f]{64}", report["legacy_identity"])
            or not isinstance(report.get("pipeline_digest"), str) or not re.fullmatch(r"[0-9a-f]{64}", report["pipeline_digest"])):
        raise RuntimeError("Single-image embedded inference smoke contract differs")
    return report


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


# Admin/UI production inputs embedded in airk (split §10.5.4): sources, manifests, locks,
# the build script, generated contracts and the operations table build.mjs reads. The sealed
# bundle crates/airk-cli/admin-assets is already under "crates". No Node toolchain pin file
# exists (the pin is the workflow node-version); receipts record the Node/npm actually used.
FRONTEND_INPUTS = ("packages/admin", "packages/ui", "packages/contracts-gen",
                   "packages/vscode/src/uiPilotContract.ts", "packages/vscode/src/adminUiContract.ts", "packages/vscode/src/generated/i2.ts",
                   "fixtures/split/i3/admin_operations.json", "scripts/build-admin-assets.py",
                   "scripts/split_receipts.py", "scripts/standalone-installer-contract.py")


def compiled_literal_inputs(files, audit=None):
    """Conservatively seal literal Rust data includes, including test includes.

    Literal and manifest-relative forms name source bytes directly. Generated
    OUT_DIR model data is owned by airk-host/build.rs and the qualified bundled
    model catalog/provisioner; generated Admin data is owned by airk-https's
    build script and FRONTEND_INPUTS. Those generator sources are already under
    crates. Static ORT archives have the separate verified static input identity.
    This is a bounded source inventory, not a Rust macro expansion engine.
    """
    string = r'"(?:[^"\\]|\\.)*"'
    literal = re.compile(r'include_(?:str|bytes)!\s*\(\s*(' + string + r')\s*,?\s*\)')
    manifest = re.compile(r'include_(?:str|bytes)!\s*\(\s*concat!\s*\(\s*env!\s*\(\s*"CARGO_MANIFEST_DIR"\s*\)\s*,((?:\s*' + string + r'\s*,?)+)\s*\)\s*,?\s*\)')
    dependencies = set()
    for name in files:
        path = ROOT / os.fsdecode(name)
        if path.suffix != ".rs" or not path.is_file() or not path.is_relative_to(ROOT / "crates"):
            continue
        text = path.read_text(encoding="utf-8")
        if len(text.encode("utf-8")) > 4 * 1024 * 1024:
            raise RuntimeError("Rust embed source inventory exceeds file bound")
        crate = ROOT / "crates" / path.relative_to(ROOT / "crates").parts[0]
        paths = [(path.parent, json.loads(match[1])) for match in literal.finditer(text)]
        paths += [(crate, "".join(json.loads(value) for value in re.findall(string, match[1])).lstrip("/"))
                  for match in manifest.finditer(text)]
        for parent, value in paths:
            selected = parent / value
            resolved = selected.resolve()
            if selected.is_symlink() or not resolved.is_relative_to(ROOT) or not selected.is_file():
                raise RuntimeError("Compiled source input is missing or escapes the runtime workspace")
            for ancestor in selected.parents:
                if ancestor == ROOT:
                    break
                if ancestor.is_symlink():
                    raise RuntimeError("Compiled source input is redirected")
            dependencies.add(os.fsencode(resolved.relative_to(ROOT).as_posix()))
            if audit is not None:
                audit.append({"source": path.relative_to(ROOT).as_posix(),
                              "input": resolved.relative_to(ROOT).as_posix(), "owner": "runtime source seal"})
    if len(dependencies) > 16_384:
        raise RuntimeError("Compiled source dependency inventory exceeds bound")
    return dependencies


def source_identity():
    names = subprocess.check_output([
        "git", "-c", f"safe.directory={ROOT.as_posix()}", "ls-files", "-z",
        "--cached", "--others", "--exclude-standard", "--", "Cargo.toml", "Cargo.lock",
        "rust-toolchain.toml", ".cargo", "crates", "scripts/build-service-runtime.py",
        "scripts/provision-onnx-runtime.py", "scripts/provision-local-model.py", "scripts/model-build-inputs.py", "scripts/int8-policy.py",
        "scripts/provision-static-onnx-runtime.py", "scripts/static-onnx-runtime-lock.json",
        "scripts/provision-static-onnx-tools.py", "scripts/static-onnx-tools-lock.json",
        "scripts/provision-static-cmake-source.py", "scripts/static-cmake-source-lock.json",
        "scripts/adopt-neo-static-onnx-runtime.py",
        "scripts/static-onnx-link-closure.py", "scripts/static-onnx-aggregate.py",
        # airk-process::signature embeds this policy into both native images.
        "scripts/signing-identity.json", "fixtures/models/model-policy.json",
        "scripts/macho-identity.py",
        "scripts/license-notices.py", "LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md", "third_party",
        *FRONTEND_INPUTS], cwd=ROOT)
    result = hashlib.sha256()
    files = set(name for name in names.split(b"\0") if name)
    files.update(compiled_literal_inputs(files))
    files = sorted(files)
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


def write_pair_metadata(staging, version, operating_system, architecture, admin_inventory_sha256=None):
    executables = runtime_executables(operating_system, version)
    if split_pair_version(version):
        i1, i2 = interface_versions()
        manifest = {"schema": 3, "version": version, "os": operating_system, "arch": architecture,
                    "airk_sha256": digest(staging / executables[0]), "airkctl_sha256": digest(staging / executables[1]),
                    "i1_protocol": i1, "i2_contract": i2, "admin_inventory_sha256": admin_inventory_sha256}
        split_receipts().validate_pair_metadata(manifest)
        (staging / "airk-runtime.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
        for executable in executables:
            identity = subprocess.check_output(version_command(staging, executable, version),
                                               text=True, encoding="utf-8").strip()
            if identity != f"{Path(executable).stem} {version}":
                raise RuntimeError(f"Unexpected executable identity: {identity}")
        return manifest
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


def split_pair_version(version):
    """The split pair gate (`airk` service + `airkctl` CLI); one owner in split_receipts.py."""
    return split_receipts().split_version(version)


def split_images(operating_system):
    images = split_receipts().IMAGES[operating_system]
    return (images["airk"], images["airkctl"])


def runtime_executables(operating_system, version):
    if split_pair_version(version):
        return split_images(operating_system)
    names = runtime_names(operating_system)
    return names[:1] if single_image_version(version) else names


def service_executable(operating_system, version):
    """The service image by name: model checks, smoke and self-test never run on airkctl."""
    if split_pair_version(version):
        return split_images(operating_system)[0]
    return runtime_executables(operating_system, version)[0]


def version_command(staging, executable, version):
    """`--version` probe. The split service image parses arguments only behind the U93 interim role."""
    path = str(staging / executable)
    if split_pair_version(version) and Path(executable).name in ("airk", "airk.exe"):
        return [path, "__airk_role", "setup-interim", "--version"]
    return [path, "--version"]


def _rust_constant(relative, name):
    text = (ROOT / relative).read_text(encoding="utf-8")
    values = re.findall(rf"^pub const {name}: u32 = ([0-9]+);$", text, re.MULTILINE)
    if len(values) != 1:
        raise RuntimeError(f"Expected exactly one {name} in {relative}")
    return int(values[0])


def interface_versions():
    """(I1 protocol, I2 contract) read from their Rust owners, which source_identity() covers."""
    return (_rust_constant("crates/airk-core/src/service.rs", "PROTOCOL_VERSION"),
            _rust_constant("crates/airkctl/src/contract/mod.rs", "I2_CONTRACT"))


def runtime_metadata_matches(metadata, version, operating_system, architecture, files):
    """Strict versioned decoder: absence of a legacy helper is never inferred."""
    names = runtime_executables(operating_system, version)
    if split_pair_version(version):
        try:
            split_receipts().validate_pair_metadata(metadata)
        except ValueError:
            return False
        return (all(files.get(name) for name in names) and metadata["version"] == version
                and (metadata["os"], metadata["arch"]) == (operating_system, architecture)
                and (metadata["airk_sha256"], metadata["airkctl_sha256"]) == (files[names[0]], files[names[1]]))
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
            "ProgramArguments": (["airk", "__airk_role", "service-login-current"]
                                 if tuple(map(int, version.split("."))) >= (1, 9, 0)
                                 else ["airk", "runtime", entry, "--channel", channel]),
            "RunAtLoad": True, "KeepAlive": {"SuccessfulExit": False},
            "ThrottleInterval": 30, "ProcessType": "Background",
        }, sort_keys=True)
    return files


def arrange_macos_bundle(staging, version):
    executables = runtime_executables("macos", version)
    sources = ("airk", "airkctl") if split_pair_version(version) else ("airk", "airk-embed")
    for source, destination in zip(sources, executables):
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
    # The split CLI is nested code: sign it before sealing the bundle.
    cli = staging / "AIRK.app/Contents/MacOS/airkctl"
    if cli.exists():
        subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", "--options", "0",
                        "--timestamp=none", str(cli)], check=True)
    image = staging / "AIRK.app/Contents/MacOS/airk-launcher-image"
    if image.exists():
        subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", "--options", "0",
                        "--timestamp=none", str(image)], check=True)
    subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", "--options", "0",
                    "--timestamp=none", str(staging / "AIRK.app")], check=True)
    subprocess.run(["/usr/bin/codesign", "--verify", "--strict", str(staging / "AIRK.app")], check=True)


def qualify_staged_runtime(staging, version, operating_system, architecture, profile,
                           source, static_inputs, admin_inventory=None, admin_block=None,
                           observe_source=None, development_macos_signing=False, before_seal=None):
    """Qualify and seal the actual staged images, without invoking Cargo.

    Every producer freezes inputs before compiling and passes those observations
    here. Signing precedes this boundary, so metadata and import/model probes
    describe the exact signed copies that packaging consumes. Containers supply
    their bounded COPY-input observer instead of inventing Git provenance.
    """
    observe_source = source_identity if observe_source is None else observe_source
    split = split_pair_version(version)
    if split and (admin_inventory is None or admin_block is None):
        raise RuntimeError("Split staging requires the checked Admin inventory")
    # These are the producer's post-signing copies, before any probe can run.
    # Metadata must describe exactly these bytes, not a replacement introduced
    # between a successful probe and the later metadata/hash observation.
    images = {name: digest(staging / name) for name in runtime_executables(operating_system, version)}
    verify_embedded_models(staging, operating_system, version)
    if observe_source() != source:
        raise RuntimeError("Native source changed during the runtime build; staging was not published")
    if development_macos_signing and operating_system == "macos":
        sign_development_macos_bundle(staging)
        # The canonical developer-Mac route signs here, before import/smoke
        # probes; its final metadata intentionally identifies those signed bytes.
        images = {name: digest(staging / name) for name in images}
    imports = verify_static_imports(staging / service_executable(operating_system, version), operating_system)
    cli_imports = (verify_cli_imports(staging / runtime_executables(operating_system, version)[1], operating_system)
                   if split else None)
    if verify_static_inputs() != static_inputs:
        raise RuntimeError("Static ORT compiled inputs changed during the build")
    stage_static_notices(staging, static_inputs)
    if any(digest(staging / name) != expected for name, expected in images.items()):
        raise RuntimeError("Staged runtime images changed during qualification")
    if split:
        manifest = write_pair_metadata(staging, version, operating_system, architecture, admin_block["inventory_sha256"])
    else:
        manifest = write_pair_metadata(staging, version, operating_system, architecture)
    if any(digest(staging / name) != expected for name, expected in images.items()):
        raise RuntimeError("Staged runtime images changed during metadata publication")
    # Exercise the final executable receipt, not a receipt-less development
    # directory. A runtime that cannot consume its own emitted metadata fails.
    model_smoke = verify_model_smoke(staging, operating_system, version)
    if observe_source() != source:
        raise RuntimeError("Native source changed during the paired build; staging was not published")
    rustc = subprocess.check_output(["rustc", "--version", "--verbose"], text=True, encoding="utf-8").strip()
    if split:
        if load_script("build-admin-assets").block_from_inventory(admin_inventory) != admin_block:
            raise RuntimeError("Admin bundle changed during the build; staging was not published")
        receipt = {"schema": 2, "source": source, "profile": profile, "runtime": manifest, "rustc": rustc,
                   "static_ort": static_inputs, "static_imports": imports, "airkctl_static_imports": cli_imports,
                   "embedded_model_smoke": model_smoke, "admin_assets": admin_block}
        split_receipts().validate_build_receipt(receipt)
    else:
        receipt = {"schema": 1, "source": source, "profile": profile,
                   "runtime": manifest, "rustc": rustc,
                   "static_ort": static_inputs, "static_imports": imports, "embedded_model_smoke": model_smoke}
    receipt_text = json.dumps(receipt, indent=2) + "\n"
    if len(receipt_text.encode("utf-8")) > 16_384:
        raise RuntimeError("Complete build receipt exceeds the native metadata bound")
    (staging / "airk-build.json").write_text(receipt_text, encoding="utf-8")
    if before_seal is not None:
        before_seal(receipt)
    seal_install_payload(staging, version, operating_system, architecture)
    if observe_source() != source:
        raise RuntimeError("Native source changed during offline payload staging")
    if (verify_static_inputs() != static_inputs
            or split and load_script("build-admin-assets").block_from_inventory(admin_inventory) != admin_block):
        raise RuntimeError("Static ORT or Admin inputs changed during offline payload staging")
    if any(digest(staging / name) != expected for name, expected in images.items()):
        raise RuntimeError("Staged runtime images changed during offline payload staging")
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--profile", choices=["debug", "release", "release-local"], default="debug")
    parser.add_argument("--admin-inventory", type=Path, default=ROOT / "artifacts/admin/admin-assets.json",
                        help="Split versions: the inventory scripts/build-admin-assets.py wrote after its checks")
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
    split = split_pair_version(version)
    admin_block = None
    if split:
        # The sealed admin bundle is verified before compiling: the checked inventory must
        # equal the committed bundle (a stale or edited bundle stops the build).
        if not args.admin_inventory.is_file():
            raise SystemExit("Split builds need the admin inventory; run scripts/build-admin-assets.py first")
        admin = load_script("build-admin-assets")
        admin_inventory = json.loads(args.admin_inventory.read_text(encoding="utf-8"))
        admin_block = admin.block_from_inventory(admin_inventory)
        check_lightweight_dependencies()
    source = source_identity()
    load_script("int8-policy").catalog()
    static_inputs = verify_static_inputs()
    with tempfile.TemporaryDirectory(prefix=".airk-runtime-stage-", dir=destination.parent) as temporary:
        staging = Path(temporary)
        main_binary = cargo_binary("airk", args.profile, "local-inference")
        shutil.copy2(main_binary, staging / main_binary.name)
        if split:
            cli_binary = cargo_binary("airkctl", args.profile, None, package="airkctl")
            shutil.copy2(cli_binary, staging / cli_binary.name)
        operating_system = {"Windows": "windows", "Darwin": "macos", "Linux": "linux"}.get(platform.system())
        architecture = {"amd64": "x86_64", "x86_64": "x86_64", "arm64": "aarch64", "aarch64": "aarch64"}.get(platform.machine().lower())
        if not operating_system or not architecture:
            raise RuntimeError("Unsupported host runtime pair target")
        if operating_system == "macos":
            arrange_macos_bundle(staging, version)
        qualify_staged_runtime(staging, version, operating_system, architecture, args.profile,
                               source, static_inputs, admin_inventory if split else None, admin_block,
                               development_macos_signing=True)
        # The final directory is created only after both artifacts and identities verify.
        publish_staging(staging, destination)
    print(f"Staged {version} {args.profile} runtime: {destination}")


if __name__ == "__main__":
    main()
