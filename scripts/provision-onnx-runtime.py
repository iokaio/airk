# SPDX-License-Identifier: Apache-2.0
"""Explicit build-time provisioning of pinned CPU libraries; never an install-time download."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import tempfile
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
CATALOG = json.loads((ROOT / "crates/airk-retrieval/src/ort_runtime_catalog.json").read_text())


def host_platform():
    operating_system = {"Windows": "windows", "Darwin": "macos", "Linux": "linux"}[platform.system()]
    arch = {"amd64": "x86_64", "x86_64": "x86_64", "arm64": "aarch64", "aarch64": "aarch64"}[platform.machine().lower()]
    return operating_system + "-" + arch


def unlinked(path):
    path = Path(path).absolute()
    for ancestor in (path, *path.parents):
        if ancestor.is_symlink() or (hasattr(ancestor, "is_junction") and ancestor.is_junction()):
            raise ValueError("Runtime provisioning path is linked")
    return path


def verify(path, size, digest):
    path = unlinked(path)
    if not path.is_file() or path.stat().st_size != size:
        raise ValueError("Runtime asset size/type differs: " + path.name)
    with path.open("rb") as source:
        actual = hashlib.file_digest(source, "sha256").hexdigest()
    if actual != digest:
        raise ValueError("Runtime asset digest differs: " + path.name)


def entries(target):
    if CATALOG["schema"] != 1 or target not in CATALOG["platforms"]:
        raise ValueError("Unsupported ONNX Runtime target")
    result = CATALOG["platforms"][target] + CATALOG["notices"]
    names = set()
    for row in result:
        if (row["name"] in names or Path(row["name"]).name != row["name"]
                or "/" in row["name"] or "\\" in row["name"]
                or not 0 < row["bytes"] <= 64 * 1024 * 1024
                or len(row["sha256"]) != 64):
            raise ValueError("Invalid pinned runtime asset")
        names.add(row["name"])
    return result


def provision(destination, target, archive=None, download=False):
    rows = entries(target)
    destination = unlinked(destination)
    # Complete existing directories need no archive and never use the network.
    if all((destination / row["name"]).exists() for row in rows):
        for row in rows:
            verify(destination / row["name"], row["bytes"], row["sha256"])
        return rows
    archive = unlinked(archive or ROOT / "target/onnx-runtime-archives/Microsoft.ML.OnnxRuntime.1.22.1.nupkg")
    if not archive.exists():
        if not download:
            raise ValueError("Missing offline runtime assets; build maintainer must supply them or explicitly use --download")
        archive.parent.mkdir(parents=True, exist_ok=True)
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(dir=archive.parent, prefix="ort-download-", delete=False) as output:
                temporary = Path(output.name)
                with urllib.request.urlopen(CATALOG["url"], timeout=120) as source:
                    count = 0
                    while block := source.read(1024 * 1024):
                        count += len(block)
                        if count > CATALOG["archive_bytes"]:
                            raise ValueError("Runtime archive exceeds pinned size")
                        output.write(block)
                output.flush()
                os.fsync(output.fileno())
            verify(temporary, CATALOG["archive_bytes"], CATALOG["archive_sha256"])
            with archive.open("xb") as output, temporary.open("rb") as source:
                while block := source.read(1024 * 1024):
                    output.write(block)
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)
    verify(archive, CATALOG["archive_bytes"], CATALOG["archive_sha256"])
    destination.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(archive) as zipped:
        inventory = zipped.namelist()
        for row in rows:
            path = destination / row["name"]
            if path.exists():
                verify(path, row["bytes"], row["sha256"])
                continue
            if inventory.count(row["source"]) != 1:
                raise ValueError("Runtime archive has missing/duplicate selected asset")
            info = zipped.getinfo(row["source"])
            if info.file_size != row["bytes"]:
                raise ValueError("Runtime archive asset size differs")
            content = zipped.read(info)
            if hashlib.sha256(content).hexdigest() != row["sha256"]:
                raise ValueError("Runtime archive asset digest differs")
            with path.open("xb") as output:
                output.write(content)
                output.flush()
                os.fsync(output.fileno())
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", choices=sorted(CATALOG["platforms"]), default=host_platform())
    parser.add_argument("--destination", type=Path)
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--download", action="store_true", help="Explicit build-time public asset download")
    args = parser.parse_args()
    destination = args.destination or Path(os.environ.get("AIRK_BUNDLED_RUNTIME_DIR", ROOT / "target/local-onnx-runtime")) / args.platform
    rows = provision(destination, args.platform, args.archive, args.download)
    print(json.dumps({"verified": True, "platform": args.platform, "directory": str(destination),
                      "distribution_version": CATALOG["distribution_version"], "files": rows}, indent=2))


if __name__ == "__main__":
    main()
