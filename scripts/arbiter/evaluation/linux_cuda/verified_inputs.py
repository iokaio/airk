# SPDX-License-Identifier: Apache-2.0
"""Authenticate every staged CUDA input against the original NVIDIA archive members."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import pwd
import stat
import tarfile

from graph_proof import BASE, file_hash
from unpack import guarded, member_destination

ACQUISITION = "07f177d5faf4b365d1d1a9351b62b8123e96572cc6a2362fb1cec44375fb6b5c"


def identity():
    uid=os.getuid()
    if (uid,os.geteuid(),os.getgid(),pwd.getpwuid(uid).pw_name)!=(1000,1000,1000,"tyler"):
        raise ValueError("registered non-root user required")
    return uid


def verify(root, rows):
    uid=identity()
    guarded(root,uid,stat.S_ISDIR)
    current=set()
    for path in root.rglob("*"):
        if not path.is_symlink() and path.is_dir():
            guarded(path,uid,stat.S_ISDIR)
            continue
        name=path.relative_to(root).as_posix()
        row=rows.get(name)
        if row is None or name in current:
            raise ValueError("unexpected CUDA input file: "+name)
        current.add(name)
        if "link" in row:
            guarded(path,uid,stat.S_ISLNK)
            if os.readlink(path)!=row["link"] or not path.resolve(strict=True).is_relative_to(root):
                raise ValueError("CUDA input symlink changed: "+name)
        else:
            guarded(path,uid,stat.S_ISREG)
            if path.stat().st_size!=row["bytes"] or file_hash(path)!=row["sha256"]:
                raise ValueError("CUDA input bytes changed: "+name)
    if current != set(rows):
        raise ValueError("CUDA input files missing from installed tree")


def collect(acquisition, root):
    identity()
    if file_hash(acquisition)!=ACQUISITION or root!=BASE/"cuda-build-inputs-r1":
        raise ValueError("reviewed acquisition and staging root required")
    rows={}
    packages=json.loads(acquisition.read_bytes())["archives"]
    if len(packages)!=12:
        raise ValueError("unexpected CUDA package count")
    for package in packages:
        name=package["name"]
        archive=acquisition.parent/name
        component=name.split("-linux-x86_64-",1)[0]
        with archive.open("rb") as stream:
            if os.fstat(stream.fileno()).st_size!=package["bytes"] or hashlib.file_digest(stream,"sha256").hexdigest()!=package["sha256"]:
                raise ValueError("CUDA publisher archive differs")
            stream.seek(0)
            with tarfile.open(fileobj=stream,mode="r|xz") as tar:
                for member in tar:
                    path=member_destination(root,component,name.removesuffix(".tar.xz"),member.name)
                    if path is None or member.isdir():
                        continue
                    key=path.relative_to(root).as_posix()
                    if member.issym():
                        row={"link":member.linkname}
                    elif member.isfile() and 0<=member.size<=2<<30:
                        with tar.extractfile(member) as source:
                            row={"bytes":member.size,"sha256":hashlib.file_digest(source,"sha256").hexdigest()}
                    else:
                        raise ValueError("unsupported publisher member")
                    if key in rows and rows[key]!=row:
                        raise ValueError("distinct publisher members map to one file")
                    rows[key]=row
    verify(root,rows)
    return {"acquisition_sha256":ACQUISITION,"staging_root":str(root),"files":rows}


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--acquisition",type=Path,required=True)
    p.add_argument("--inputs",type=Path,required=True)
    a=p.parse_args()
    print(json.dumps(collect(a.acquisition,a.inputs),sort_keys=True))
