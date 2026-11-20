# SPDX-License-Identifier: Apache-2.0
"""Check whether current files and actual Git blobs reconstruct the stock CPU pair's original source census."""
import hashlib
import itertools
import json
import os
from pathlib import Path
import runpy
import subprocess

ROOT=Path(__file__).resolve().parents[3]
E=ROOT/"artifacts/v190/final-plan/exec-20261116T052942Z"
BUILT=E/"windows-fresh-cpu-product-r2/runtime/airk-build.json"


def git(*args):
    return subprocess.check_output(["git",*args],cwd=ROOT)


def main():
    source=runpy.run_path(str(ROOT/"scripts/build-service-runtime.py"))
    observe=source["source_identity"]
    original=observe.__globals__["digest"]
    rows={}
    def capture(path):
        val=original(path)
        rows[path.relative_to(ROOT).as_posix()]=val
        return val
    observe.__globals__["digest"]=capture
    current=observe()
    required=json.loads(BUILT.read_bytes())["source"]
    if len(rows)!=current["files"] or current["files"]!=required["files"]:
        raise ValueError("actual stock census contains missing or different files")
    commit=git("rev-parse","695bb7bc^{commit}").decode().strip()
    changed=git("diff","--name-only","-z",commit,"--","crates","scripts/build-service-runtime.py")
    names=[os.fsdecode(k) for k in changed.split(b"\x00") if k and os.fsdecode(k) in rows]
    if len(names)>16:
        raise ValueError("too many original changed inputs for this limited inventory")
    options={}
    for name in names:
        exists=subprocess.run(["git","cat-file","-e",f"{commit}:{name}"],cwd=ROOT,capture_output=True)
        if exists.returncode!=0:
            continue
        data=git("cat-file","--filters",f"{commit}:{name}")
        sha=hashlib.sha256(data).hexdigest()
        if sha!=rows[name]:
            options[name]=sha
    ordered=sorted(rows)
    def census(selected):
        digest=hashlib.sha256()
        for name in ordered:
            key=os.fsencode(name)
            digest.update(len(key).to_bytes(8,"big"))
            digest.update(key)
            digest.update(bytes.fromhex(options[name] if name in selected else rows[name]))
        return digest.hexdigest()
    if census(set())!=current["sha256"]:
        raise ValueError("independent original stock input reconstruction differs")
    changes=list(options)
    match=[]
    for n in range(len(changes)+1):
        for selection in itertools.combinations(changes,n):
            if census(set(selection))==required["sha256"]:
                match.append({name:options[name] for name in selection})
    print(json.dumps({"current":current,"required":required,"git_commit":commit,"alternative_blobs":options,
                      "matches":match,"raw_source_files":len(rows),"candidates":2**len(changes)}))


if __name__=="__main__":
    main()
