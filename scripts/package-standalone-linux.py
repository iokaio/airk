# SPDX-License-Identifier: Apache-2.0
"""Wrap a verified Linux native candidate in an unsigned, offline CLI installer."""
import argparse
import gzip
import hashlib
import importlib.util
import json
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
TARGET = "x86_64-unknown-linux-gnu"
MAX_INSTALLER_FILES = 2052
MAX_INSTALLER_BYTES = 4_294_967_296 + (1 << 20)


def load_native():
    spec = importlib.util.spec_from_file_location("airk_standalone_linux_native", ROOT / "scripts/package-native.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


native = load_native()


def safe_name(name):
    if (not isinstance(name, str) or not name or len(name) > 4096
            or not re.fullmatch(r"[A-Za-z0-9._/-]+", name)
            or name.startswith("/") or "//" in name
            or any(part in (".", "..", "") for part in name.split("/"))
            or str(PurePosixPath(name)) != name):
        raise ValueError("Unsafe standalone installer payload path")
    return name


def candidate(directory):
    """Preserve the canonical native validator; this wrapper never rebuilds/signs."""
    directory = Path(directory).resolve(strict=True)
    receipt = native.bounded_json(directory / "manifest.json", 4 << 20)
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    archive_name = f"airk-{version}-{TARGET}.zip"
    if (receipt.get("schema") != "airk-native-candidate/2" or receipt.get("version") != version
            or receipt.get("target") != TARGET or receipt.get("signed_distribution") is not False
            or receipt.get("signing") is not None or receipt.get("archive") != archive_name
            or receipt.get("cargo_lock_sha256") != native.sha256(ROOT / "Cargo.lock")
            or receipt.get("toolchain_file_sha256") != native.sha256(ROOT / "rust-toolchain.toml")
            or type(receipt.get("dirty_checkout")) is not bool):
        raise ValueError("Linux standalone input is not the matching unsigned native candidate")
    if set(path.name for path in directory.iterdir()) != {archive_name, "manifest.json", "SHA256SUMS"}:
        raise ValueError("Unexpected native candidate files")
    native.inventory(directory)  # Refuse linked, aliased, special or unbounded inputs.
    if not isinstance(receipt.get("payload"), dict) or not 1 <= len(receipt["payload"]) <= native.MAX_PAYLOAD_FILES:
        raise ValueError("Native candidate payload roster exceeds bound")
    for name, digest in receipt["payload"].items():
        safe_name(name)
        if not isinstance(digest, str) or not re.fullmatch(r"[a-f0-9]{64}", digest):
            raise ValueError("Invalid native candidate payload digest")
    if any(name.endswith(".vsix") for name in receipt["payload"]):
        raise ValueError("A standalone installer cannot contain an editor extension")
    archive = directory / archive_name
    if native.sha256(archive) != receipt.get("archive_sha256"):
        raise ValueError("Native candidate archive digest differs")
    sums = (f"{native.sha256(archive)}  {archive_name}\n"
            f"{native.sha256(directory / 'manifest.json')}  manifest.json\n")
    if (directory / "SHA256SUMS").read_text(encoding="utf-8") != sums:
        raise ValueError("Native candidate checksums differ")
    builder = native.runtime_builder()
    if receipt.get("source") != builder.source_identity():
        raise ValueError("Native candidate differs from current implementation inputs")
    if not re.fullmatch(r"[a-f0-9]{40}", str(receipt.get("commit", ""))) or not re.fullmatch(r"[a-f0-9]{40}", str(receipt.get("tree", ""))):
        raise ValueError("Native candidate source revision is missing")
    if (receipt["commit"] != native.capture("git", "rev-parse", "HEAD")
            and receipt["tree"] != native.capture("git", "rev-parse", "HEAD^{tree}")):
        raise ValueError("Native candidate source revision differs")
    native.verify_archive(archive, receipt["payload"], "airk")
    return receipt, archive


def stage_runtime(archive, receipt, destination):
    import zipfile
    destination.mkdir(parents=True, exist_ok=False)
    with zipfile.ZipFile(archive) as zipped:
        for name in sorted(receipt["payload"]):
            info = zipped.getinfo(name)
            if stat.S_IFMT(info.external_attr >> 16) != stat.S_IFREG:
                raise ValueError("Native candidate archive contains a non-regular file")
            path = destination / safe_name(name)
            path.parent.mkdir(parents=True, exist_ok=True)
            with zipped.open(info) as source, path.open("xb") as output:
                shutil.copyfileobj(source, output, 1 << 20)
            path.chmod(0o755 if name == "airk" else 0o644)
    files, executables = native.validate_runtime(destination, receipt["version"], TARGET, receipt["source"])
    if files != receipt["payload"] or executables != ("airk",) or files.get("airk-install.json") != receipt.get("install_manifest_sha256"):
        raise ValueError("Staged Linux runtime differs from native candidate")
    return files


def render_template(name, values):
    template = (ROOT / "installers/linux" / name).read_text(encoding="utf-8")
    for key, value in values.items():
        template = template.replace("@" + key + "@", str(value))
    if re.search(r"@[A-Z_]+@", template):
        raise ValueError("Unresolved Linux installer template value")
    return template


def verify_installer(archive, expected, directory_name):
    """Bounded read-only verification used by packaging and release assembly."""
    safe_name(directory_name)
    if "/" in directory_name or not 1 <= len(expected) <= MAX_INSTALLER_FILES:
        raise ValueError("Invalid standalone installer inventory")
    for name, digest in expected.items():
        safe_name(name)
        if not re.fullmatch(r"[a-f0-9]{64}", str(digest)):
            raise ValueError("Invalid standalone installer digest")
    observed, total = set(), 0
    with tarfile.open(archive, "r|gz") as packaged:
        for member in packaged:
            if not member.name.startswith(directory_name + "/"):
                raise ValueError("Installer archive escaped its package directory")
            name = safe_name(member.name[len(directory_name) + 1:])
            total += member.size
            if (not member.isfile() or member.pax_headers or name not in expected or name in observed
                    or len(observed) >= MAX_INSTALLER_FILES or member.size < 0 or total > MAX_INSTALLER_BYTES):
                raise ValueError("Installer archive has a linked, duplicate, unexpected or oversized entry")
            mode = 0o755 if name in ("install.sh", "runtime/airk") else 0o644
            if member.mode != mode:
                raise ValueError("Installer archive executable/file mode differs")
            with packaged.extractfile(member) as source:
                if hashlib.file_digest(source, "sha256").hexdigest() != expected[name]:
                    raise ValueError("Installer archive payload digest differs")
            observed.add(name)
    if observed != set(expected):
        raise ValueError("Installer archive omits required payload")


def write_installer(stage, archive, inventory, directory_name):
    with archive.open("xb") as output, gzip.GzipFile(filename="", mode="wb", fileobj=output, mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode="w|", format=tarfile.GNU_FORMAT) as packaged:
            for name in sorted(inventory):
                path = stage / name
                member = tarfile.TarInfo(directory_name + "/" + name)
                member.size = path.stat().st_size
                member.mode = 0o755 if name in ("install.sh", "runtime/airk") else 0o644
                member.mtime = member.uid = member.gid = 0
                member.uname = member.gname = ""
                with path.open("rb") as source:
                    packaged.addfile(member, source)
    verify_installer(archive, inventory, directory_name)


def package_candidate(native_directory, output):
    source_paths = (Path(__file__).resolve(), ROOT / "installers/linux/install.sh.in", ROOT / "installers/linux/README.txt.in")
    installer_sources = {path.relative_to(ROOT).as_posix(): native.sha256(path) for path in source_paths}
    receipt, archive = candidate(native_directory)
    output = Path(output).resolve()
    if not output.is_relative_to((ROOT / "artifacts").resolve()) or output == (ROOT / "artifacts").resolve():
        raise ValueError("Standalone output must be a fresh subdirectory of artifacts/")
    output.mkdir(parents=True, exist_ok=False)
    stage, publish = output / "stage", output / "publish"
    stage.mkdir(); publish.mkdir()
    runtime_files = stage_runtime(archive, receipt, stage / "runtime")
    sums = "".join(f"{digest}  runtime/{name}\n" for name, digest in sorted(runtime_files.items()))
    (stage / "payload.sha256").write_text(sums, encoding="utf-8", newline="\n")
    directory_name = f"airk-{receipt['version']}-{TARGET}-installer"
    archive_name = directory_name + ".tar.gz"
    values = {"VERSION": receipt["version"], "ARCHIVE_NAME": archive_name,
              "DIRECTORY_NAME": directory_name, "PAYLOAD_FILE_COUNT": len(runtime_files),
              "INSTALL_MANIFEST_SHA256": runtime_files["airk-install.json"],
              "PAYLOAD_SUMS_SHA256": native.sha256(stage / "payload.sha256")}
    (stage / "install.sh").write_text(render_template("install.sh.in", values), encoding="utf-8", newline="\n")
    (stage / "README.txt").write_text(render_template("README.txt.in", values), encoding="utf-8", newline="\n")
    (stage / "install.sh").chmod(0o755)
    payload = native.inventory(stage)
    artifact = publish / archive_name
    write_installer(stage, artifact, payload, directory_name)
    installer = {"schema": "airk-standalone-installer/1", "version": receipt["version"], "target": TARGET,
                 "commit": receipt["commit"], "tree": receipt["tree"], "dirty_checkout": receipt["dirty_checkout"],
                 "cargo_lock_sha256": receipt["cargo_lock_sha256"],
                 "runtime_install_sha256": runtime_files["airk-install.json"], "runtime_source": receipt["source"],
                 "native_manifest_sha256": native.sha256(Path(native_directory) / "manifest.json"),
                 "native_archive_sha256": receipt["archive_sha256"], "channel": "stable",
                 "artifact": artifact.name, "artifact_sha256": native.sha256(artifact),
                 "installer_kind": "linux_tar", "code_signing": "unsigned",
                 "signing": {"platform": "Linux", "trust": "unsigned"},
                 "installer_sources": installer_sources,
                 "payload": payload, "editor_extension_included": False,
                 "qualification": "Packaged unsigned CLI/native server; installed Linux acceptance remains separate"}
    if installer_sources != {path.relative_to(ROOT).as_posix(): native.sha256(path) for path in source_paths}:
        raise ValueError("Linux installer sources changed during packaging")
    (publish / "installer.json").write_text(json.dumps(installer, indent=2) + "\n", encoding="utf-8", newline="\n")
    (publish / "SHA256SUMS").write_text(f"{native.sha256(artifact)}  {artifact.name}\n"
        f"{native.sha256(publish / 'installer.json')}  installer.json\n", encoding="utf-8", newline="\n")
    return installer


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native-dir", type=Path, required=True, help="Verified package-native.py publish directory")
    parser.add_argument("--output", type=Path, default=ROOT / "artifacts/standalone" / TARGET)
    args = parser.parse_args(argv)
    result = package_candidate(args.native_dir, args.output)
    print(json.dumps({key: result[key] for key in ("artifact", "artifact_sha256", "installer_kind", "signing")}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
