# SPDX-License-Identifier: Apache-2.0
"""Inventory WSL2 CUDA dependencies without loading candidate shared objects."""
import argparse
import fnmatch
import hashlib
import json
import os
from pathlib import Path
import platform
import pwd
import re
import resource
import subprocess
import sys
import time


BUILD = Path("/home/tyler/airk-builds/arbiter-final-linux-r1")
ROOTS = (Path("/usr/lib/wsl/lib"), Path("/usr/lib/x86_64-linux-gnu"),
         Path("/usr/local/lib"), Path("/opt"), BUILD / "inputs")
PATTERNS = ("libcuda.so*", "libonnxruntime_providers_cuda.so*", "libonnxruntime_providers_shared.so*",
            "libcudnn*.so*", "libcudart.so*", "libcublas*.so*")
DYNAMIC = re.compile(r"\((NEEDED|SONAME|RUNPATH|RPATH)\).*?\[([^\]]+)\]")


def dynamic_tags(text):
    result = {}
    for kind, value in DYNAMIC.findall(text):
        result.setdefault(kind, []).append(value)
    return result


def read_tool(argv):
    return subprocess.run(argv, check=True, capture_output=True, text=True, timeout=10,
                          env={"LC_ALL": "C", "PATH": "/usr/bin:/bin"}).stdout


def source_record(path):
    with path.open("rb") as src:
        before = os.fstat(src.fileno())
        prefix = src.read(20)
        src.seek(0)
        digest = hashlib.file_digest(src, "sha256").hexdigest()
        after = os.fstat(src.fileno())
    if not (len(prefix) == 20 and prefix[:6] == b"\x7fELF\x02\x01" and
            int.from_bytes(prefix[18:20], "little") == 62):
        raise ValueError("expected ELF64 x86-64 little-endian")
    tags = dynamic_tags(read_tool(["/usr/bin/readelf", "-dW", str(path)]))
    observed = path.stat()
    if not all((before.st_dev, before.st_ino, before.st_size, before.st_ctime_ns, before.st_mtime_ns) ==
               (row.st_dev, row.st_ino, row.st_size, row.st_ctime_ns, row.st_mtime_ns)
               for row in (after, observed)):
        raise ValueError("shared object changed during inventory")
    return {"bytes": before.st_size, "sha256": digest, "uid": before.st_uid, "dynamic_tags": tags}


def inventory(uid, user):
    if (os.getuid(), os.geteuid(), pwd.getpwuid(uid).pw_name, platform.machine()) != (uid, uid, user, "x86_64") or uid == 0:
        raise ValueError("registered non-root interactive user and x86_64 required")
    started = time.monotonic()
    roots = [*ROOTS, *Path("/usr/local").glob("cuda*")]
    roots = sorted({p.resolve(strict=True) for p in roots if p.is_dir()})
    names = []
    errors = []

    def note_error(error):
        errors.append({"path": error.filename, "errno": error.errno})

    for root in roots:
        for where, _, files in os.walk(root, onerror=note_error, followlinks=False):
            if time.monotonic() - started > 120:
                raise TimeoutError("Linux CUDA inventory deadline")
            for name in files:
                if any(fnmatch.fnmatchcase(name, pattern) for pattern in PATTERNS):
                    names.append(Path(where) / name)
    objects = {}
    for name in sorted(names):
        target = name.resolve(strict=True)
        key = str(target)
        if key not in objects:
            if target.is_relative_to("/usr/local/lib/ollama"):
                record = {"classification": "third_party_ollama", "bytes": target.stat().st_size}
            else:
                record = source_record(target) | {"classification": "observed_only"}
            objects[key] = record | {"aliases": []}
        objects[key]["aliases"].append(str(name))
    cache = [line.strip() for line in read_tool(["/sbin/ldconfig", "-p"]).splitlines()
             if any(fnmatch.fnmatchcase(line.strip().split(" ", 1)[0], pattern) for pattern in PATTERNS)]
    return {"schema": "airk.private.wsl-cuda-inventory/1", "uid": uid, "user": user,
            "kernel": platform.release(), "distribution": platform.freedesktop_os_release()["PRETTY_NAME"],
            "scan_roots": list(map(str, roots)), "scan_errors": errors, "loader_cache": cache,
            "objects": objects, "has_dxg_device": Path("/dev/dxg").is_char_device(),
            "nofile": resource.getrlimit(resource.RLIMIT_NOFILE),
            "providers_loaded": False, "gpu_initialized": False,
            "elapsed_seconds": time.monotonic() - started}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--uid", type=int, required=True)
    parser.add_argument("--user", required=True)
    args = parser.parse_args()
    if not (sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode):
        parser.error("use python -I -B -S")
    print(json.dumps(inventory(args.uid, args.user), sort_keys=True))
