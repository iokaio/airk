# SPDX-License-Identifier: Apache-2.0
"""Provision the offline Linux license generator from the human's existing, Cargo.lock-pinned crate archives."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import tomllib

from configure_ort import BASE, sha
from product_projection import checked_path
from shipping_build import capacity, document, space
from configure_ort import CAMPAIGN
from verified_inputs import identity


def run(args):
    uid = identity()
    os.umask(0o077)
    source = document(args.source, args.source_sha256)
    root = checked_path(Path(source["source"]), True)
    lock = checked_path(root / "Cargo.lock")
    registry = "registry/cache/index.crates.io-1949cf8c6b5b557f"
    original = Path("/mnt/c/Users/tdotn/.cargo") / registry
    target = checked_path(BASE / "cargo" / registry, True)
    if original.resolve(strict=True) != original or original.stat().st_uid != uid:
        raise ValueError("unexpected source Cargo cache")
    pinned = {}
    for row in tomllib.loads(lock.read_text())["package"]:
        if row.get("source") != "registry+https://github.com/rust-lang/crates.io-index":
            continue
        name = row["name"] + "-" + row["version"] + ".crate"
        if not re.fullmatch(r"[a-zA-Z0-9_.+\-]+", name) or not re.fullmatch(r"[0-9a-f]{64}",row["checksum"]):
            raise ValueError("unexpected registry package identity")
        pinned[name] = row["checksum"]
    needed = {name:digest for name,digest in pinned.items() if not (target/name).exists()}
    total = 0
    for name,digest in needed.items():
        file = original/name
        meta = file.lstat()
        if (not stat.S_ISREG(meta.st_mode) or meta.st_uid != uid or file.resolve() != file
                or meta.st_size > 64 << 20 or sha(file) != digest):
            raise ValueError("original crate differs from Cargo.lock: " + name)
        total += meta.st_size
    baseline = json.loads((CAMPAIGN/"linux-cuda-r4-provider-r3-space-watch.jsonl").read_text().splitlines()[0])
    if total > 1 << 30 or not capacity(baseline,space(),total):
        raise ValueError("offline cache admission exceeds the original guest/backing reservation")
    for name,digest in needed.items():
        dst = target/name
        with (original/name).open("rb") as src, dst.open("xb") as output:
            shutil.copyfileobj(src,output)
        checked_path(dst)
        if sha(dst) != digest:
            raise ValueError("native provisioned crate checksum differs: "+name)
    return {"source":str(original),"target":str(target),"uid":uid,"lock_sha256":sha(lock),"archives":needed,
            "added_bytes":total,"registry_packages":len(pinned),"source_receipt":args.source_sha256}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source",type=Path,required=True)
    parser.add_argument("--source-sha256",required=True)
    print(json.dumps(run(parser.parse_args()),sort_keys=True))
