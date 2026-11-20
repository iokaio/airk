# SPDX-License-Identifier: Apache-2.0
"""Stage only original AWS revision-pinned documents in an inert, path-preserving private ZIP."""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import re
import tarfile
import time
from urllib.request import Request, urlopen
from zipfile import ZIP_STORED, ZipFile, ZipInfo

ROOT=Path(__file__).resolve().parents[4]
CAMPAIGN=ROOT/"artifacts/v190/final-plan/exec-20261116T052942Z"
INVENTORY=CAMPAIGN/"aws-natural-menu-inventory-r1.private.json"
INVENTORY_SHA="adea1f245f326d7814c011eb9538df1d9fd4343cec2cfc50aedbb45885831c65"
REV="2d06ee9011f3192b2ff38c09f04e01f1ea9e0191"
URL="https://codeload.github.com/siagholami/aws-documentation/tar.gz/"+REV
RIGHTS=re.compile(r"^(?:licen[cs]e(?:[-_.](?:summary|samplecode|mit|apache|bsd))?|notice|copying|copyright|third[-_]party[-_]notices?)(?:[.](?:txt|md|rst))?$",re.I)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


class Bounded:
    def __init__(self, stream, deadline):
        self.stream=stream
        self.deadline=deadline
        self.total=0
        self.digest=hashlib.sha256()

    def read(self,n):
        if time.monotonic()>=self.deadline:
            raise TimeoutError("original archive acquisition deadline expired")
        data=self.stream.read(n)
        self.total+=len(data)
        if self.total>256<<20:
            raise ValueError("compressed publisher archive exceeds bound")
        self.digest.update(data)
        return data


def identity(raw, record):
    git=hashlib.sha1(f"blob {len(raw)}\0".encode()+raw,usedforsecurity=False).hexdigest()
    if len(raw)!=record["bytes"] or git!=record["git_blob"]:
        raise ValueError("publisher member bytes differ from complete Git tree")
    return sha(raw)


def stage(output):
    if output.parent!=CAMPAIGN or not output.is_dir() or any(output.iterdir()):
        raise ValueError("fresh checked private corpus output required")
    raw=INVENTORY.read_bytes()
    if sha(raw)!=INVENTORY_SHA:
        raise ValueError("unrecognized full publisher tree inventory")
    inv=json.loads(raw)
    entries=inv["source_files"]
    if len(entries)!=35515 or sum(r["bytes"] for r in entries.values())!=263271678:
        raise ValueError("unrecognized original corpus population")
    expected_prefix="aws-documentation-"+REV+"/"
    files={}
    rights={}
    summaries={}
    started=time.monotonic()
    archive=output/"documents.private.zip"
    request=Request(URL,headers={"User-Agent":"AIRK-pinned-source-audit"})
    with urlopen(request,timeout=30) as conn, archive.open("xb") as dst, ZipFile(dst,"w",ZIP_STORED) as out:
        bounded=Bounded(conn,started+600)
        with tarfile.open(fileobj=bounded,mode="r|gz") as src:
            for member in src:
                path=member.name.removeprefix(expected_prefix)
                if member.isdir() or not path.startswith("documents/"):
                    continue
                if member.name!=expected_prefix+path or path not in entries or path in files:
                    raise ValueError("unexpected or duplicate publisher path")
                record=entries[path]
                if record["mode"]=="120000" and member.issym():
                    contents=member.linkname.encode("utf-8")
                elif record["mode"] in ("100644","100755") and member.isfile() and member.size==record["bytes"]<=32<<20:
                    with src.extractfile(member) as file:
                        contents=file.read(member.size+1)
                else:
                    raise ValueError("unsupported publisher member type or size")
                files[path]=identity(contents,record)
                parts=path.split("/")
                if RIGHTS.fullmatch(parts[-1]) and len(contents)<=128<<10:
                    rights[path]={"sha256":files[path],"text":contents.decode("utf-8-sig",errors="replace")}
                if len(parts)==3 and parts[-1].lower()=="readme.md":
                    text=contents.decode("utf-8-sig",errors="replace")
                    summary=re.search(r"(?im)^#{1,4}\s+(?:license(?: summary)?|licensing)\s*$",text)
                    if summary:
                        trailing=text[summary.end():].strip()
                        summaries[path]={"sha256":files[path],"license_section":re.split(r"(?m)^#{1,4}\s",trailing)[0][:4096]}
                info=ZipInfo(path)
                info.external_attr=0o100600<<16
                out.writestr(info,contents)
        while bounded.read(1<<16):
            pass
    if set(files)!=set(entries):
        raise ValueError("archive omits original source paths")
    with archive.open("rb") as file:
        archive_sha=hashlib.file_digest(file,"sha256").hexdigest()
    receipt={"schema":"airk.private.aws-full-source-pack/1","inventory_sha256":INVENTORY_SHA,"commit":REV,
             "archive_url":URL,"archive_compressed_bytes":bounded.total,"source_archive_sha256":bounded.digest.hexdigest(),
             "pack_sha256":archive_sha,"pack_bytes":archive.stat().st_size,"files":files,"rights_materials":rights,
             "readme_license_sections":summaries,"modes":dict(Counter(e["mode"] for e in entries.values())),
             "elapsed_seconds":time.monotonic()-started,"scored_requests":0,"native_visibility_measured":False,
             "qa_csv_in_pack":False}
    with (output/"pack.private.json").open("x",encoding="utf-8") as out:
        json.dump(receipt,out,sort_keys=True,separators=(",",":"))
    return {key: receipt[key] for key in ("pack_sha256","pack_bytes","archive_compressed_bytes","modes","elapsed_seconds")}


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--output",type=Path,required=True)
    print(json.dumps(stage(p.parse_args().output),sort_keys=True))
