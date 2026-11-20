# SPDX-License-Identifier: Apache-2.0
"""Observe exact ELF dependency closure from the staged Linux CUDA publisher images."""
import argparse
import json
import os
from pathlib import Path
import pwd
import stat
import time

from inventory import source_record
from unpack import guarded


def inventory(root, shared):
    uid = os.getuid()
    if uid != 1000 or os.geteuid() != uid or pwd.getpwuid(uid).pw_name != "tyler":
        raise ValueError("expected registered non-root interactive user")
    start = time.monotonic()
    for p in (root.parent.parent.parent, root.parent.parent, root.parent, root):
        guarded(p, uid, stat.S_ISDIR)
    libraries = [root / "cuda/lib", root / "cudnn/lib"]
    system = [Path("/usr/lib/wsl/lib"), Path("/usr/lib/x86_64-linux-gnu")]
    # Verify paths/metadata only. No object is loaded or executed.
    objects = {}
    pending = {}
    for lib in libraries:
        guarded(lib, uid, stat.S_ISDIR)
        for path in sorted(lib.glob("*.so*")):
            resolved = path.resolve(strict=True)
            if not resolved.is_relative_to(lib):
                raise ValueError("publisher shared object escapes isolated input")
            guarded(resolved, uid, stat.S_ISREG)
            if resolved not in objects:
                row = source_record(resolved)
                row["source"] = "publisher"
                objects[resolved] = row
    guarded(shared, uid, stat.S_ISREG)
    for path, kind in [(shared, "local-build"), (system[0] / "libcuda.so.1", "host-driver")]:
        real = path.resolve(strict=True)
        objects[real] = dict(source_record(real), source=kind)
    names = {}
    def index(path, row):
        soname = row["dynamic_tags"].get("SONAME", [])
        if len(soname) != 1 or (soname[0] in names and names[soname[0]] != path):
            raise ValueError("missing or ambiguous ELF SONAME: " + str(path))
        names[soname[0]] = path
    for path, row in objects.items():
        index(path, row)
    for row in objects.values():
        for dep in row["dynamic_tags"].get("NEEDED", []):
            pending[dep] = True
    while pending:
        name, _ = pending.popitem()
        if name in names:
            continue
        if "/" in name or name in ("", ".", ".."):
            raise ValueError("invalid system SONAME")
        hits = {p.resolve(strict=True) for d in system if (p := d / name).is_file()}
        if len(hits) != 1:
            raise ValueError("system ELF not resolved uniquely in trusted directories: " + name)
        path = hits.pop()
        if not any(path.is_relative_to(d) for d in system):
            raise ValueError("system ELF escaped trusted canonical root: " + str(path))
        for entry in [path, *path.parents]:
            st = entry.stat()
            if st.st_uid or st.st_mode & 0o022:
                raise ValueError("untrusted source of system ELF at " + str(entry))
        row = dict(source_record(path), source="host-dependency")
        objects[path] = row
        index(path, row)
        if name not in names:
            raise ValueError("system ELF SONAME mismatch")
        for dep in row["dynamic_tags"].get("NEEDED", []):
            pending[dep] = True
    return {"schema": "airk.private.linux-cuda-staged-elf-inventory/1",
            "objects": {str(path): data for path, data in sorted(objects.items())},
            "elapsed_seconds": time.monotonic()-start}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inputs", type=Path, required=True)
    parser.add_argument("--shared", type=Path, required=True)
    options = parser.parse_args()
    print(json.dumps(inventory(options.inputs, options.shared), sort_keys=True))
