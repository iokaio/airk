# SPDX-License-Identifier: Apache-2.0
"""Materialize one source-only local Git view for stock installer consume; never run Cargo build."""
import hashlib
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[2]
E=ROOT/"artifacts/v190/final-plan/exec-20261116T052942Z"
VIEW=E/"iv"
PROOF=E/"windows-stock-local-installer-git-source-r1.json"
EXPECTED="1b4129081eed3c839ffe9999bc1feb1ea650704e2ec2280180d0cd681005ea6d"


def require(ok,msg):
    if not ok:
        raise ValueError(msg)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def git(*args,cwd=ROOT):
    return subprocess.check_output(["git",*args],cwd=cwd)


def main():
    require(os.environ.get("AIRK_CHECKED_INSTALLER_SOURCE_VIEW")==str(VIEW) and VIEW.is_dir() and not any(VIEW.iterdir()),
            "fresh checked D source output required")
    require(digest(PROOF.read_bytes())=="93366cd9ac6027c87bcf8278748ead372558b7da98c82149e030a7db30e7b5b7","wrong source proof")
    proof=json.loads(PROOF.read_bytes())
    replaced,=proof["matches"]
    commit=proof["git_commit"]
    build=runpy.run_path(str(ROOT/"scripts/build-service-runtime.py"))
    observe=build["source_identity"]
    original=observe.__globals__["digest"]
    rows={}
    def capture(path):
        value=original(path)
        rows[path.relative_to(ROOT).as_posix()]=value
        return value
    observe.__globals__["digest"]=capture
    observe()
    require(len(rows)==2109,"input inventory changed")
    sources=set(rows)
    for name in ("third_party","installers/windows"):
        sources.update(os.fsdecode(n) for n in git("ls-files","-z","--",name).split(b"\0") if n)
    sources.update(p.relative_to(ROOT).as_posix() for p in (ROOT/"scripts").iterdir() if p.is_file() and p.suffix in (".py",".json"))
    sources.update((".gitignore",".gitattributes","LICENSE","NOTICE","THIRD_PARTY_NOTICES.md"))
    sources={n for n in sources if (ROOT/n).is_file()}
    material={}
    for name in sorted(sources):
        source=ROOT/name
        require(not source.is_symlink() and source.resolve(strict=True).is_relative_to(ROOT),"redirected source")
        data=git("cat-file","--filters",f"{commit}:{name}") if name in replaced else source.read_bytes()
        sha=digest(data)
        if name in rows:
            require(sha==replaced.get(name,rows[name]),"source changed while freezing")
            rows[name]=sha
        material[name]=data
    check=hashlib.sha256()
    for name in sorted(rows):
        key=os.fsencode(name)
        check.update(len(key).to_bytes(8,"big")+key+bytes.fromhex(rows[name]))
    require(check.hexdigest()==EXPECTED and sum(map(len,material.values()))<256<<20,"source closure or source cap differs")
    # Object alternates are read-only; the ref, index and all generated files belong to this private view.
    git("clone","--shared","--no-checkout","--no-tags",str(ROOT),str(VIEW))
    git("update-ref","--no-deref","HEAD",commit,cwd=VIEW)
    git("read-tree",commit,cwd=VIEW)
    require(git("rev-parse","HEAD",cwd=VIEW).decode().strip()==commit,"source view HEAD differs")
    for name,data in material.items():
        path=VIEW/name
        require(path.resolve().is_relative_to(VIEW),"projected path escaped private Git checkout")
        path.parent.mkdir(parents=True,exist_ok=True)
        with path.open("xb") as output:
            output.write(data)
    actual=runpy.run_path(str(VIEW/"scripts/build-service-runtime.py"))["source_identity"]()
    require(actual==proof["required"],"unmodified stock producer did not observe the original input census")
    print(json.dumps({"root":str(VIEW),"source":actual,"commit":commit,"original_git_files":replaced,
                      "files":len(material),"bytes":sum(map(len,material.values())),
                      "support":{name:digest(data) for name,data in material.items() if name not in rows}}))


if __name__=="__main__":
    main()
