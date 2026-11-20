# SPDX-License-Identifier: Apache-2.0
"""Prepare a source-only, exact one-pin proposal for the private native CPU regression."""
import argparse
import hashlib
import json
import re
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[4]
POLICY = Path("crates/airk-retrieval/src/arbiter/runtime/cpu_source_transition.rs")
RUNTIME = Path("crates/airk-retrieval/src/arbiter/runtime.rs")
OLD = "52563c3185ff061dc852fbdd0f12846a6b4f04330ad2f66ddec9c8b5afa77252"
PROPOSED = "7b4650767610eafdfea4b6c273e042008e4eae4654dde516d438c6ee2f8449b3"
SOURCE_LIMIT = 96 << 20


def sha(data):
    return hashlib.sha256(data).hexdigest()


def inputs():
    raw = subprocess.check_output(["git","ls-files","-c","-o","--exclude-standard","-z","--",
        "Cargo.toml","Cargo.lock","rust-toolchain.toml",".cargo",".config","third_party","crates","fixtures"],
        cwd=ROOT)
    paths = sorted({Path(name.decode("utf-8")) for name in raw.split(b"\0") if name})
    return [p for p in paths if (ROOT/p).is_file()]


def original(path):
    file = ROOT/path
    if file.resolve(strict=True) != file or file.is_symlink():
        raise ValueError("noncanonical source")
    return file.read_bytes()


def candidate(path, data):
    if path != POLICY:
        return data
    literal = f'Some("{OLD}")'.encode()
    if data.count(literal) != 1:
        raise ValueError("one exact closed source pin required")
    return data.replace(literal, f'Some("{PROPOSED}")'.encode())


def referenced_inputs(dest):
    """Discover Rust static includes against the frozen checkout, including multiline literals."""
    refs = set()
    for source in (dest/"crates").rglob("*.rs"):
        for literal in re.findall(r'include_(?:str|bytes)!\(\s*"([^"]+)"\s*\)',source.read_text("utf-8")):
            absolute = (source.parent/literal).resolve()
            if not absolute.is_relative_to(dest):
                raise ValueError("static include escapes the snapshot")
            rel = absolute.relative_to(dest)
            if rel.parts[0] not in {"crates","fixtures","third_party"}:
                refs.add(rel)
    return sorted(refs)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("action",choices=("plan","copy","verify","includes-plan","includes-copy"))
    p.add_argument("destination",type=Path)
    args = p.parse_args()
    dest = args.destination
    if dest.parent != ROOT/"artifacts/v190/final-plan/exec-20261116T052942Z" or dest.name != "legacy-focus-source-proposal-r1":
        raise ValueError("unexpected private source destination")
    if args.action.startswith("includes-"):
        rows = referenced_inputs(dest)
        for row in rows:
            if row.parts[0] not in {"docs","packages","scripts","tools"} or row.suffix != ".json":
                raise ValueError("unexpected cross-repository static include")
    else:
        rows = inputs()
    if sha(original(RUNTIME).replace(b"\r\n",b"\n")) != PROPOSED:
        raise ValueError("reviewed source epoch changed")
    if sha(original(POLICY).replace(b"\r\n",b"\n")) != "530c1d7fdca73bff5114e3d1e880dc36a7715002573b58edfeca76fda84d8a41":
        raise ValueError("original transition owner changed")
    allocated = sum(((ROOT/row).stat().st_size + 4095)//4096*4096 for row in rows)
    if allocated > SOURCE_LIMIT:
        raise ValueError("source copy exceeds reservation")
    if args.action in {"plan","includes-plan"}:
        print(json.dumps({"files":[str(row) for row in rows],"rounded_source_bytes":allocated}))
        return
    manifest = []
    for row in rows:
        source = original(row)
        output = candidate(row,source)
        path = dest/row
        if path.parent.resolve(strict=True) != path.parent or path.is_symlink():
            raise ValueError("noncanonical destination")
        if args.action in {"copy","includes-copy"}:
            with path.open("xb") as f:
                f.write(output)
        if path.read_bytes() != output:
            raise ValueError("source copy differs: "+str(row))
        manifest.append({"path":row.as_posix(),"original":sha(source),"candidate":sha(output)})
    print(json.dumps({"sources":manifest,"rounded_source_bytes":allocated,
        "old_runtime_sha256":OLD,"private_proposed_runtime_sha256":PROPOSED},sort_keys=True))


if __name__ == "__main__":
    main()
