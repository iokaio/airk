# SPDX-License-Identifier: Apache-2.0
"""Authenticate NVIDIA archive identities and stage Linux CUDA build inputs."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import time
from urllib.request import urlopen


BASE = "https://developer.download.nvidia.com/compute/"
INDEXES = {
    "cuda": ("12.8.1", "249e28a83008d711d5f72880541c8be6253f6d61608461de4fcb715554a6cf17",
             ("cuda_cccl", "cuda_cudart", "cuda_nvcc", "cuda_nvrtc", "cuda_nvtx", "cuda_cupti",
              "cuda_profiler_api", "libcublas", "libcurand", "libcufft", "libnvjitlink")),
    "cudnn": ("9.10.2", "73a33a12bbb8eb12b105a515b5921db2e328b3ca679f92b6184c7f32fe94a8b0", ("cudnn",)),
}


def selected(family, data):
    version, expected, names = INDEXES[family]
    if hashlib.sha256(data).hexdigest() != expected:
        raise ValueError("NVIDIA release metadata digest differs")
    metadata = json.loads(data)
    if metadata["release_label"] != version:
        raise ValueError("release identity differs")
    rows = []
    for name in names:
        package = metadata[name]["linux-x86_64"]
        if family == "cudnn":
            package = package["cuda12"]
        relative = package["relative_path"]
        if not re.fullmatch(rf"{name}/linux-x86_64/{name}-linux-x86_64-[A-Za-z0-9_.-]+-archive\.tar\.xz", relative):
            raise ValueError("unexpected source archive path")
        size = int(package["size"])
        if not (0 < size < 2 << 30 and re.fullmatch("[a-f0-9]{64}", package["sha256"])):
            raise ValueError("invalid archive size or digest")
        rows.append({"name": relative.split("/")[-1], "url": BASE + family + "/redist/" + relative,
                     "bytes": size, "sha256": package["sha256"]})
    return rows


def plan():
    rows = []
    for family, (version, _, _) in INDEXES.items():
        url = f"{BASE}{family}/redist/redistrib_{version}.json"
        with urlopen(url, timeout=30) as response:
            if response.geturl() != url:
                raise ValueError("unexpected metadata redirect")
            rows.extend(selected(family, response.read(2 << 20)))
    return rows


def verify(path, row):
    if path.is_symlink() or path.stat().st_size != row["bytes"]:
        raise ValueError("staged archive kind or size differs")
    with path.open("rb") as stream:
        if hashlib.file_digest(stream, "sha256").hexdigest() != row["sha256"]:
            raise ValueError("staged archive digest differs")


def stage(directory, row):
    path = directory / row["name"]
    if path.exists():
        verify(path, row)
        return
    partial = path.with_suffix(".partial")
    start = time.monotonic()
    with urlopen(row["url"], timeout=45) as response, partial.open("xb") as out:
        if response.geturl() != row["url"]:
            raise ValueError("unexpected archive redirect")
        left = row["bytes"]
        while left:
            if time.monotonic() - start > 1800:
                raise TimeoutError("per-archive acquisition deadline")
            block = response.read(min(left, 1 << 20))
            if not block:
                raise ValueError("archive ended before its declared size")
            out.write(block)
            left -= len(block)
        if response.read(1):
            raise ValueError("archive exceeds declared size")
    verify(partial, row)
    partial.rename(path)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", type=Path, help="prechecked private directory; default emits only the plan")
    args = parser.parse_args()
    rows = plan()
    if args.stage:
        if not args.stage.is_absolute() or args.stage.resolve(strict=True) != args.stage or not args.stage.is_dir():
            parser.error("use a canonical, existing, prechecked private directory")
        for row in rows:
            stage(args.stage, row)
    print(json.dumps({"source_indexes": {k: v[1] for k, v in INDEXES.items()}, "archives": rows,
                      "staged": bool(args.stage)}, sort_keys=True))
