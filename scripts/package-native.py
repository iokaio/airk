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
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parent.parent
MAX_PAYLOAD_BYTES = 4_294_967_296
MAX_PAYLOAD_FILES = 2049
# Rust triple -> pre-split VSIX platform. Split versions use only the triples (the VSIX is
# universal, U29); historical readers keep the platform values.
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
    builder = runtime_builder()
    split = builder.split_pair_version(version)
    for metadata in (install, pair):
        expected_schema = (3 if split else 2 if builder.single_image_version(version) else 1) if metadata is pair else 1
        if metadata.get("schema") != expected_schema or metadata.get("version") != version or metadata.get("os") != operating_system or metadata.get("arch") != architecture:
            raise ValueError("Runtime pair platform/version differs")
    expected = {name: value for name, value in files.items() if name != "airk-install.json"}
    if install.get("files") != expected:
        raise ValueError("Runtime install inventory differs from complete payload")
    if build.get("schema") != (2 if split else 1) or build.get("source") != source or build.get("runtime") != pair:
        raise ValueError("Runtime build receipt differs from current source/pair")
    executables = builder.runtime_executables(operating_system, version)
    if split:
        # Both images, by name: a payload missing either one (including airkctl without airk) is refused.
        missing = [name for name in executables if name not in files]
        if missing:
            raise ValueError("Runtime pair executable missing: " + ", ".join(missing))
        try:
            builder.split_receipts().validate_build_receipt(build)
        except ValueError as error:
            raise ValueError("Runtime build receipt differs from the split receipt contract: " + str(error)) from error
    main = executables[0]
    launcher = builder.launcher_name(operating_system, version)
    if launcher != main and launcher not in files:
        raise ValueError("Runtime standalone launcher image is missing")
    allowed = {*executables, launcher, "airk-install.json", "airk-runtime.json", "airk-build.json", "LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md"}
    required = allowed | {"notices/inventory.json", "notices/bom.cdx.json", "notices/all-MiniLM-L6-v2/LICENSE", "notices/all-MiniLM-L6-v2/provenance.json"}
    if tuple(int(part) for part in version.split(".")) >= (1, 9, 0):
        # The legal producer owns the exact companion names and authenticated pins.
        # Historical candidates retain their original two-file notice contract.
        legal = builder.load_script("license-notices")
        model_prefix = "notices/all-MiniLM-L6-v2/"
        model_files = {model_prefix + name: digest
                       for name, digest in legal.MODEL_COMPANIONS.values()}
        model_files[model_prefix + "provenance.json"] = sha256(
            ROOT / "crates/airk-retrieval/src/model_catalog.json")
        required.update(model_files)
        for name, digest in model_files.items():
            if files.get(name) != digest:
                raise ValueError("Pinned trained-model notice differs or is missing: " + name)
    if not required.issubset(files):
        raise ValueError("Runtime pair legal/notice inventory is incomplete")
    allowed = set(required)
    if builder.single_image_version(version):
        static_notices = bounded_json(runtime / "notices/static-onnx-runtime/inventory.json")
        allowed.update(builder.static_notice_inventory(static_notices, target, files))
    inference_assets = builder.inference_assets(operating_system, architecture, version) if builder.int8_version(version) else {}
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
    if not builder.runtime_metadata_matches(pair, version, operating_system, architecture, files):
        raise ValueError("Runtime pair executable digests differ")
    return files, tuple(dict.fromkeys((*executables, launcher)))


def split_candidate(common, files, executables, runtime, model_smoke):
    """`airk-native-candidate/3` (split C7): both image digests, the admin binding from the
    validated build receipt and the interface versions; no `vsix_target` (U29)."""
    pair = bounded_json(runtime / "airk-runtime.json", 4096)
    build = bounded_json(runtime / "airk-build.json", 65_536)
    receipt = {"schema": "airk-native-candidate/3", **common,
               "airk_sha256": files[executables[0]], "airkctl_sha256": files[executables[1]],
               "admin_assets": {"inventory_sha256": build["admin_assets"]["inventory_sha256"],
                                "bundle_sha256": build["admin_assets"]["bundle_sha256"]},
               "i1_protocol": pair["i1_protocol"], "i2_contract": pair["i2_contract"],
               "embedded_model_smoke": model_smoke,
               "qualification": "native service pair candidate; installed acceptance and oldest OS not qualified"}
    runtime_builder().split_receipts().validate_native_candidate(receipt)
    return receipt


def source_packaging_metadata(path, target, signed):
    receipt = bounded_json(path, 4_194_304)
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    # A split pair is repackaged only from its own tested airk-native-candidate/3 receipt. The
    # C7 owner is loaded beside this file (not through ROOT, which tests may relocate).
    specification = importlib.util.spec_from_file_location("airk_split_receipts", Path(__file__).with_name("split_receipts.py"))
    receipts = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(receipts)
    schema = "airk-native-candidate/3" if receipts.split_version(version) else "airk-native-candidate/2"
    if (not signed or receipt.get("schema") != schema
            or receipt.get("version") != version or receipt.get("target") != target
            or receipt.get("dirty_checkout") is not False or receipt.get("signed_distribution") is not False
            or receipt.get("cargo_lock_sha256") != sha256(ROOT / "Cargo.lock")
            or receipt.get("toolchain_file_sha256") != sha256(ROOT / "rust-toolchain.toml")
            or receipt.get("tree") != capture("git", "rev-parse", "HEAD^{tree}")
            or not isinstance(receipt.get("rustc"), str)
            or "host: " + target not in receipt["rustc"].splitlines()
            or not isinstance(receipt.get("cargo"), str) or not receipt["cargo"].startswith("cargo ")):
        raise ValueError("Source packaging metadata differs from this tested tree/target")
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument("--output", type=Path, help="Fresh directory inside ignored artifacts/; defaults to artifacts/native/<target>")
    parser.add_argument("--runtime-dir", type=Path, required=True, help="Exact tested build-service-runtime.py output")
    parser.add_argument("--signed-dir", type=Path, help="Fresh sign-native.py artifact copy and receipt, verified again on this OS")
    parser.add_argument("--source-manifest", type=Path, help="Reuse verified CI compiler metadata for signed repackaging")
    parser.add_argument("--signtool", default="signtool")
    args = parser.parse_args()
    source_manifest = None
    if args.source_manifest:
        source_manifest = source_packaging_metadata(args.source_manifest, args.target, bool(args.signed_dir))
        compiler, version = source_manifest["rustc"], source_manifest["version"]
    else:
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
    executable = executables[0]
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
    operating_system = "windows" if "windows" in args.target else "macos" if "apple" in args.target else "linux"
    split = builder.split_pair_version(version)
    for name in executables:
        (smoke / name).chmod(0o755)
        identity = "airk" if name.endswith("/airk-launcher-image") else Path(name).stem
        if capture(*builder.version_command(smoke, name, version)) != identity + " " + version:
            raise SystemExit("Archived executable version differs")
    service = builder.service_executable(operating_system, version)
    self_test = ([str(smoke / service), "__airk_role", "setup-interim", "runtime", "self-test"] if split
                 else [str(smoke / executable), "runtime", "self-test"])
    result = json.loads(capture(*self_test))
    if result.get("self_test") != "passed" or result.get("version") != version:
        raise SystemExit("Archived native service pair self-test failed")
    catalog = json.loads((ROOT / "crates/airk-retrieval/src/model_catalog.json").read_text(encoding="utf-8"))
    embedded = builder.verify_embedded_models(smoke, operating_system, version)
    model_smoke = builder.verify_model_smoke(smoke, operating_system, version) if builder.single_image_version(version) else None
    if split:
        # The split service answers `model-catalog` only over its control endpoint (no running
        # service here), so the offline smoke selects the profile; the source catalog names its manifest.
        selected = model_smoke["profile"]
        profile = next((p for p in catalog["profiles"] if p["id"] == selected), None)
        if profile is None:
            raise SystemExit("Archived inference probe selected a profile outside the source catalog")
        bundled_model = {"manifest": profile["manifest"]}
    else:
        model_report = json.loads(capture(str(smoke / executable), "model-catalog"))
        selected = model_report.get("selection", {}).get("profile_id")
        profile = next((p for p in catalog["profiles"] if p["id"] == selected), None)
        if profile is None or model_report.get("bundled_model", {}).get("manifest") != profile["manifest"]:
            raise SystemExit("Archived model metadata differs from source catalog")
        bundled_model = model_report["bundled_model"]
    if model_smoke is not None and model_smoke["profile"] != selected:
        raise SystemExit("Archived inference probe selected a different CPU model profile")
    common = {
        "version": version, "target": args.target,
        "commit": capture("git", "rev-parse", "HEAD"),
        "tree": capture("git", "rev-parse", "HEAD^{tree}"),
        "dirty_checkout": bool(capture("git", "status", "--porcelain")),
        "source": builder.source_identity(),
        "cargo_lock_sha256": sha256(ROOT / "Cargo.lock"),
        "toolchain_file_sha256": sha256(ROOT / "rust-toolchain.toml"),
        "rustc": compiler, "cargo": source_manifest["cargo"] if source_manifest else capture("cargo", "--version"),
        "archive": archive.name, "archive_sha256": sha256(archive),
        "install_manifest_sha256": files["airk-install.json"], "payload": files,
        "bundled_model": bundled_model, "embedded_models": embedded,
        "signed_distribution": signing is not None, "signing": signing,
    }
    if split:
        receipt = split_candidate(common, files, executables, runtime, model_smoke)
    else:
        # Pre-split /2 keeps its original key order (its manifest bytes are digested downstream).
        order = ("version", "bundled_model", "embedded_models", "source", "install_manifest_sha256", "target",
                 "vsix_target", "commit", "tree", "dirty_checkout", "cargo_lock_sha256", "toolchain_file_sha256",
                 "rustc", "cargo", "archive", "archive_sha256", "payload", "signed_distribution", "signing")
        values = {**common, "vsix_target": TARGETS[args.target]}
        receipt = {"schema": "airk-native-candidate/2", **{key: values[key] for key in order},
                   "qualification": "native CLI candidate; installed editor and oldest OS not qualified"}
        if model_smoke is not None:
            receipt["embedded_model_smoke"] = model_smoke
    (publish / "manifest.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    (publish / "SHA256SUMS").write_text(
        sha256(archive) + "  " + archive.name + "\n" +
        sha256(publish / "manifest.json") + "  manifest.json\n", encoding="utf-8")
    print("Verified native candidate: " + str(archive))


if __name__ == "__main__":
    main()
