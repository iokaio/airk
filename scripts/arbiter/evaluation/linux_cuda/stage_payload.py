# SPDX-License-Identifier: Apache-2.0
"""Stage the exact Linux payload roster emitted by the Rust owner and prepare native assembly input."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import pwd
import shutil
import stat
import subprocess

from authenticated_build import CUDA
from configure_ort import BASE, CAMPAIGN, audit, sha
from finish_ort import checked_receipt
from inventory import source_record
from unpack import guarded

NOTICES = "e6ff31964e2bf806e4d184c6dd44f4bdbfbb3cb960c784605ed509a389f927a8"
IMAGES = "49df2bc67cc4cb95964e36399b503bc28d19e3c3cc6a8086cdd5ac4f37df3359"


def load_order(payload, system, last):
    pending = dict(payload)
    done = set(system)
    ordered = []
    while pending:
        ready = sorted(n for n, row in pending.items() if set(row["dynamic_tags"].get("NEEDED",[])) <= done
                       and (n != last or len(pending) == 1))
        if not ready:
            raise ValueError("Linux runtime has unresolved/cyclic ELF dependencies")
        for name in ready:
            ordered.append(name)
            done.add(name)
            del pending[name]
    return ordered


def write_checked(out, data, uid):
    guarded(out.parent, uid, stat.S_ISDIR)
    with out.open("xb") as f:
        f.write(data)
    guarded(out, uid, stat.S_ISREG)
    if sha(out) != hashlib.sha256(data).hexdigest():
        raise ValueError("staged metadata mismatch")


def copy_checked(src, dest, expected, uid):
    guarded(src, uid, stat.S_ISREG)
    guarded(dest.parent, uid, stat.S_ISDIR)
    with src.open("rb") as f, dest.open("xb") as output:
        shutil.copyfileobj(f, output, 2 << 20)
    guarded(dest, uid, stat.S_ISREG)
    if sha(dest) != expected:
        raise ValueError("staged Linux asset differs: " + dest.name)


def run(args):
    uid = os.getuid()
    if uid != 1000 or os.geteuid() != uid or pwd.getpwuid(uid).pw_name != "tyler":
        raise ValueError("registered Linux human required")
    os.umask(0o077)
    for p in (BASE.parents[1],BASE.parent,BASE):
        guarded(p, uid, stat.S_ISDIR)
    if not args.example.is_absolute() or not args.example.is_relative_to(BASE/"target"):
        raise ValueError("expected Linux Cargo example under authorized target")
    guarded(args.example,uid,stat.S_ISREG)
    if sha(args.example) != args.example_sha256:
        raise ValueError("unreviewed Linux native producer example")
    proc = lambda argv: subprocess.run([str(args.example), *argv], env={"PATH":"/usr/bin:/bin","LC_ALL":"C"},
                                        check=True, capture_output=True, timeout=30).stdout
    package = BASE/"linux-cuda-package-r1"
    runtime = package/"runtime"
    names = proc(["--payload-names"]).decode().splitlines()
    if not 2 <= len(names) <= 64 or len(set(names)) != len(names) or any(Path(n).name != n for n in names):
        raise ValueError("invalid native Rust payload roster")
    original = checked_receipt("linux-cuda-original-notices-r2.json",NOTICES)
    holder = checked_receipt("linux-cuda-ort-final-provider-r1.json",args.provider_receipt)
    if not holder["success"]:
        raise ValueError("provider is not a successful native output")
    if args.phase == "images":
        if os.path.lexists(package):
            raise ValueError("fresh package staging root required")
        inventory = checked_receipt("linux-cuda-staged-elf-r1.json", IMAGES)
        observed = {row["dynamic_tags"]["SONAME"][0]:(Path(path),row) for path,row in inventory["objects"].items()}
        local = BASE/"linux-cuda-ort-r4/b/Release"
        for key in ("shared","provider"):
            row = holder[key]
            name = row["dynamic_tags"]["SONAME"][0]
            observed[name] = (local/name,row)
        dxcore = Path("/usr/lib/wsl/lib/libdxcore.so")
        dx = source_record(dxcore)
        if dx["uid"] != 0 or dx["sha256"] != "83d1671a839bcf71709349e77cd68341515df2e63080765618876996a0f4190c":
            raise ValueError("unreviewed WSL root-owned graphics companion")
        observed[dxcore.name] = (dxcore, dx)
        vendor = checked_receipt("linux-cuda-authenticated-inputs-r1.json", CUDA)
        vendor_root = BASE / "cuda-build-inputs-r1"
        for name in set(names) - {holder[k]["dynamic_tags"]["SONAME"][0] for k in ("shared", "provider")}:
            path, row = observed[name]
            original_row = vendor["files"][path.relative_to(vendor_root).as_posix()]
            if original_row != {"sha256":row["sha256"], "bytes":row["bytes"]}:
                raise ValueError("vendor ELF differs from original archive: " + name)
        payload = {n:observed[n][1] for n in names}
        pending = {"libcuda.so.1",dxcore.name}
        for row in payload.values():
            pending.update(set(row["dynamic_tags"].get("NEEDED",[])) - set(payload))
        system = {}
        while pending:
            n = pending.pop()
            if n in system:
                continue
            path, row = observed[n]
            if row["uid"] != 0 or not path.is_relative_to("/usr/lib"):
                raise ValueError("unexpected host ELF")
            if source_record(path) != {k:v for k,v in row.items() if k!="source"}:
                raise ValueError("host ELF changed: "+n)
            system[n] = row
            pending.update(row["dynamic_tags"].get("NEEDED",[]))
        order = load_order(payload,system,"libonnxruntime_providers_cuda.so")
        package.mkdir(0o700)
        runtime.mkdir(0o700)
        for n in order:
            p, record = observed[n]
            copy_checked(p,runtime/n,record["sha256"],uid)
        for row in original["originals"]:
            name = row["staged_name"]
            if Path(name).name != name:
                raise ValueError("unexpected legal asset location")
            copy_checked(BASE/"linux-cuda-notices-r2"/name,runtime/name,row["sha256"],uid)
        write_checked(package/"load-order.txt",("\n".join(order)+"\n").encode(),uid)
        write_checked(package/"system-imports.txt",("\n".join(sorted(system))+"\n").encode(),uid)
        audit(package,uid)
        return {"example":args.example_sha256,"runtime":str(runtime),"payload":payload,
                "system":system,"notices":len(original["originals"]),"load_order":order}
    guarded(package,uid,stat.S_ISDIR)
    guarded(runtime,uid,stat.S_ISDIR)
    core = checked_receipt("linux-cuda-ort-final-core-r1.json",args.core_receipt)
    if not core["success"]:
        raise ValueError("static build incomplete")
    files = ["linux-cuda-ort-final-core-r1.json","linux-cuda-ort-final-provider-r1.json",
             "linux-cuda-ort-configure-r4.json","linux-cuda-bootstrap-compile-r2.json",
             "fresh-cuda-configure-cu15-r1.json","linux-cuda-build-inputs-r1.json","linux-cuda-original-notices-r2.json"]
    public = runtime/"airk-linux-cuda-build-provenance.json"
    write_checked(public,proc([str(CAMPAIGN/f) for f in files]),uid)
    audit(package,uid)
    return {"example":args.example_sha256,"provenance":sha(public),"runtime":str(runtime),
            "module_names":names,"load_order":(package/"load-order.txt").read_text().splitlines(),
            "system_imports":(package/"system-imports.txt").read_text().splitlines()}


if __name__ == "__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--phase",choices=("images","provenance"),required=True)
    p.add_argument("--example",type=Path,required=True)
    p.add_argument("--example-sha256",required=True)
    p.add_argument("--provider-receipt",required=True)
    p.add_argument("--core-receipt")
    print(json.dumps(run(p.parse_args()),sort_keys=True))
