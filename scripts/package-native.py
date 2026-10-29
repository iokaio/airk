# SPDX-License-Identifier: Apache-2.0
"""Package an already-tested native service pair without rebuilding its payload."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import zipfile

ROOT = Path(__file__).resolve().parent.parent
MAX_PAYLOAD_BYTES = 4_294_967_296
MAX_PAYLOAD_FILES = 2049
TARGETS = {
    "x86_64-unknown-linux-gnu": "linux-x64",
    "x86_64-pc-windows-msvc": "win32-x64",
    "aarch64-apple-darwin": "darwin-arm64",
    "x86_64-apple-darwin": "darwin-x64",
}


def runtime_builder():
    specification = importlib.util.spec_from_file_location("airk_pair_builder", ROOT / "scripts/build-service-runtime.py")
    builder = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(builder)
    return builder


def sha256(path):
    digest = hashlib.sha256()
    count = 0
    with path.open("rb") as source:
        while block := source.read(1_048_576):
            count += len(block)
            if count > MAX_PAYLOAD_BYTES:
                raise ValueError("Runtime file exceeds installer budget")
            digest.update(block)
    return digest.hexdigest()


def bounded_json(path, maximum=524_288):
    metadata = path.lstat()
    if (not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1
            or getattr(metadata, "st_file_attributes", 0) & 0x400 or metadata.st_size > maximum):
        raise ValueError("Linked, redirected or oversized runtime metadata")
    with path.open("rb") as source:
        data = source.read(maximum + 1)
    if len(data) > maximum:
        raise ValueError("Runtime metadata grew beyond allowance")
    return json.loads(data)


def capture(*command):
    return subprocess.check_output(command, cwd=ROOT, text=True, encoding="utf-8").strip()


def inventory(stage):
    result = {}
    pending = [stage]
    total = entries = 0
    while pending:
        parent = pending.pop()
        metadata = parent.lstat()
        if not stat.S_ISDIR(metadata.st_mode) or getattr(metadata, "st_file_attributes", 0) & 0x400:
            raise ValueError("Payload directory links are forbidden")
        with os.scandir(parent) as children:
            for entry in children:
                entries += 1
                if entries > 4096:
                    raise ValueError("Runtime topology exceeds installer budget")
                path = Path(entry.path)
                # Windows DirEntry's cached find-data may report st_nlink=0;
                # obtain actual file metadata before checking aliases.
                metadata = path.lstat()
                if stat.S_ISLNK(metadata.st_mode) or getattr(metadata, "st_file_attributes", 0) & 0x400:
                    raise ValueError("Payload links are forbidden")
                if stat.S_ISDIR(metadata.st_mode):
                    pending.append(path)
                    continue
                if not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1:
                    raise ValueError("Aliased or special payload file is forbidden")
                total += metadata.st_size
                if len(result) >= MAX_PAYLOAD_FILES or total > MAX_PAYLOAD_BYTES:
                    raise ValueError("Runtime payload exceeds installer budget")
                result[path.relative_to(stage).as_posix()] = sha256(path)
    return result


def verify_archive(archive, expected, executable):
    executables = (executable,) if isinstance(executable, str) else tuple(executable)
    with zipfile.ZipFile(archive) as zipped:
        names = zipped.namelist()
        if len(names) != len(set(names)) or set(names) != set(expected):
            raise ValueError("Archive entries differ from payload inventory")
        for name, digest in expected.items():
            if zipped.getinfo(name).file_size > MAX_PAYLOAD_BYTES:
                raise ValueError("Archive file exceeds installer budget")
            with zipped.open(name) as source:
                if hashlib.file_digest(source, "sha256").hexdigest() != digest:
                    raise ValueError("Archive payload digest mismatch: " + name)
        for name in executables:
            if not (zipped.getinfo(name).external_attr >> 16) & stat.S_IXUSR:
                raise ValueError("Archive lost executable permission: " + name)


def write_archive(stage, archive, expected, executable):
    executables = (executable,) if isinstance(executable, str) else tuple(executable)
    with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED) as zipped:
        for name in sorted(expected):
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.external_attr = (stat.S_IFREG | (0o755 if name in executables else 0o644)) << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            with (stage / name).open("rb") as source, zipped.open(info, "w") as destination:
                shutil.copyfileobj(source, destination, length=1_048_576)
    verify_archive(archive, expected, executable)


def validate_runtime(runtime, version, target, source):
    files = inventory(runtime)
    install = bounded_json(runtime / "airk-install.json")
    pair = bounded_json(runtime / "airk-runtime.json", 4096)
    build = bounded_json(runtime / "airk-build.json", 65_536)
    operating_system = "windows" if "windows" in target else "macos" if "apple" in target else "linux"
    architecture = "aarch64" if target.startswith("aarch64-") else "x86_64"
    for metadata in (install, pair):
        if metadata.get("schema") != 1 or metadata.get("version") != version or metadata.get("os") != operating_system or metadata.get("arch") != architecture:
            raise ValueError("Runtime pair platform/version differs")
    expected = {name: value for name, value in files.items() if name != "airk-install.json"}
    if install.get("files") != expected:
        raise ValueError("Runtime install inventory differs from complete payload")
    if build.get("schema") != 1 or build.get("source") != source or build.get("runtime") != pair:
        raise ValueError("Runtime build receipt differs from current source/pair")
    builder = runtime_builder()
    main, helper = builder.runtime_names(operating_system)
    allowed = {main, helper, "airk-install.json", "airk-runtime.json", "airk-build.json", "LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md"}
    required = allowed | {"notices/inventory.json", "notices/bom.cdx.json", "notices/all-MiniLM-L6-v2/LICENSE", "notices/all-MiniLM-L6-v2/provenance.json"}
    if not required.issubset(files):
        raise ValueError("Runtime pair legal/notice inventory is incomplete")
    allowed = set(required)
    inference_assets = builder.inference_assets(operating_system, architecture) if builder.int8_version(version) else {}
    for name, row in inference_assets.items():
        if operating_system == "macos" and "macho_payload_sha256" in row:
            metadata = (runtime / name).stat()
            if metadata.st_size > 64 << 20 or builder.load_script("macho-identity").payload_digest((runtime / name).read_bytes()) != row["macho_payload_sha256"]:
                raise ValueError("Signed inference library content differs")
        elif files.get(name) != row["sha256"]:
            raise ValueError("Pinned offline inference library/notice differs: " + name)
        allowed.add(name)
    if operating_system == "macos":
        for name, expected in builder.macos_bundle_files(version).items():
            if (runtime / name).stat().st_size != len(expected) or (runtime / name).read_bytes() != expected:
                raise ValueError("Mac service bundle resources differ")
            allowed.add(name)
        signature = "AIRK.app/Contents/_CodeSignature/CodeResources"
        if signature in files:
            allowed.add(signature)
        ticket = "AIRK.app/Contents/CodeResources"
        if ticket in files:
            allowed.add(ticket)
    notices = bounded_json(runtime / "notices/inventory.json")
    if not isinstance(notices, list) or len(notices) > 1024:
        raise ValueError("Runtime dependency notice inventory exceeds budget")
    for notice in notices:
        directory = "notices/" + notice["name"] + "-" + notice["version"] + "/"
        names = notice["license_files"] or ["UPSTREAM-LICENSE-DECLARATION.json"]
        for name in names:
            if not name or "/" in name or "\\" in name or name in (".", ".."):
                raise ValueError("Unsafe dependency license filename")
            allowed.add(directory + name)
    if set(files) != allowed:
        raise ValueError("Unexpected runtime payload outside native/legal allowlist")
    if pair.get("airk_sha256") != files.get(main) or pair.get("embed_sha256") != files.get(helper) or not files.get(main) or not files.get(helper):
        raise ValueError("Runtime pair executable digests differ")
    return files, (main, helper)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument("--output", type=Path, help="Fresh directory inside ignored artifacts/; defaults to artifacts/native/<target>")
    parser.add_argument("--runtime-dir", type=Path, required=True, help="Exact tested build-service-runtime.py output")
    parser.add_argument("--signed-dir", type=Path, help="Fresh sign-native.py artifact copy and receipt, verified again on this OS")
    parser.add_argument("--signtool", default="signtool")
    args = parser.parse_args()
    compiler = capture("rustc", "-vV")
    if "host: " + args.target not in compiler.splitlines():
        raise SystemExit("Package and smoke-test on the intended native runner")
    metadata = json.loads(capture("cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"))
    version = next(p["version"] for p in metadata["packages"] if p["name"] == "airk-cli")
    builder = runtime_builder()
    runtime = args.runtime_dir.resolve(strict=True)
    if runtime.is_symlink() or not runtime.is_dir():
        raise SystemExit("Expected an immutable tested runtime pair directory")
    files, executables = validate_runtime(runtime, version, args.target, builder.source_identity())
    signing = None
    if args.signed_dir:
        specification = importlib.util.spec_from_file_location("airk_signing", ROOT / "scripts/sign-native.py")
        signer = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(signer)
        runtime, signing = signer.read_verified_pair(args.signed_dir.resolve(strict=True), runtime, args.target,
                                                     capture("git", "rev-parse", "HEAD"), sha256(ROOT / "Cargo.lock"), args.signtool)
        files, executables = validate_runtime(runtime, version, args.target, builder.source_identity())
    executable, helper = executables
    output = (args.output or ROOT / "artifacts" / "native" / args.target).resolve()
    if not output.is_relative_to((ROOT / "artifacts").resolve()) or output == (ROOT / "artifacts").resolve():
        raise SystemExit("Candidate output must be a fresh subdirectory of artifacts/")
    # Fresh staging prevents stale files from entering a new candidate.
    output.mkdir(parents=True, exist_ok=False)
    stage = output / "stage"
    publish = output / "publish"
    stage.mkdir()
    publish.mkdir()
    for name in files:
        destination = stage / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(runtime / name, destination)
    for name in executables:
        (stage / name).chmod(0o755)
    if inventory(stage) != files:
        raise SystemExit("Runtime payload changed during staging")
    archive = publish / ("airk-" + version + "-" + args.target + ".zip")
    write_archive(stage, archive, files, executables)
    smoke = output / "smoke"
    smoke.mkdir()
    with zipfile.ZipFile(archive) as zipped:
        # The already validated inventory supplies every extraction destination.
        for name in files:
            destination = smoke / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            with zipped.open(name) as source, destination.open("xb") as output_file:
                shutil.copyfileobj(source, output_file, length=1_048_576)
    for name in executables:
        (smoke / name).chmod(0o755)
        if capture(str(smoke / name), "--version") != Path(name).stem + " " + version:
            raise SystemExit("Archived executable version differs")
    result = json.loads(capture(str(smoke / executable), "runtime", "self-test"))
    if result.get("self_test") != "passed" or result.get("version") != version:
        raise SystemExit("Archived native service pair self-test failed")
    model_report = json.loads(capture(str(smoke / executable), "model-catalog"))
    catalog = json.loads((ROOT / "crates/airk-retrieval/src/model_catalog.json").read_text(encoding="utf-8"))
    selected = model_report.get("selection", {}).get("profile_id")
    profile = next((p for p in catalog["profiles"] if p["id"] == selected), None)
    if profile is None or model_report.get("bundled_model", {}).get("manifest") != profile["manifest"]:
        raise SystemExit("Archived model metadata differs from source catalog")
    embedded = builder.verify_embedded_models(smoke, "windows" if "windows" in args.target else "macos" if "apple" in args.target else "linux")
    receipt = {
        "schema": "airk-native-candidate/2", "version": version,
        "bundled_model": model_report["bundled_model"],
        "embedded_models": embedded,
        "source": builder.source_identity(), "install_manifest_sha256": files["airk-install.json"],
        "target": args.target, "vsix_target": TARGETS[args.target],
        "commit": capture("git", "rev-parse", "HEAD"),
        "dirty_checkout": bool(capture("git", "status", "--porcelain")),
        "cargo_lock_sha256": sha256(ROOT / "Cargo.lock"),
        "toolchain_file_sha256": sha256(ROOT / "rust-toolchain.toml"),
        "rustc": compiler, "cargo": capture("cargo", "--version"),
        "archive": archive.name, "archive_sha256": sha256(archive),
        "payload": files, "signed_distribution": signing is not None, "signing": signing,
        "qualification": "native CLI candidate; installed editor and oldest OS not qualified",
    }
    (publish / "manifest.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    (publish / "SHA256SUMS").write_text(
        sha256(archive) + "  " + archive.name + "\n" +
        sha256(publish / "manifest.json") + "  manifest.json\n", encoding="utf-8")
    print("Verified native candidate: " + str(archive))


if __name__ == "__main__":
    main()
