# SPDX-License-Identifier: Apache-2.0
"""Reopen the original ORT source roster and its six explicitly approved Linux projections."""
import argparse
import hashlib
import json
from pathlib import Path
import stat
import zipfile

from configure_ort import BUILD, CAMPAIGN, CONFIG_SHA, sha, source_path


def match(tree, expected, excluded=None):
    files={}
    for path in tree.rglob("*"):
        if excluded is not None and path.is_relative_to(excluded):
            continue
        if path.is_symlink():
            raise ValueError("unexpected source symlink: "+str(path))
        if path.is_file():
            name=path.relative_to(tree).as_posix()
            digest=sha(path)
            if expected.get(name)!=digest:
                raise ValueError("changed compiled source: "+name)
            files[name]=digest
    if set(files)!=set(expected):
        raise ValueError("compiled source roster changed")
    return {"files":len(files),"sha256":hashlib.sha256(json.dumps(files,sort_keys=True,separators=(",",":")).encode()).hexdigest()}


def verify(root, cfg):
    spec=CAMPAIGN/"fresh-cuda-configure-cu15-r1.json"
    if sha(spec)!=CONFIG_SHA or sha(root/"source-files.json")!=cfg["source"]["baseline"]:
        raise ValueError("unreviewed ORT source/mirror inventory")
    recipe=json.loads(spec.read_bytes())
    original={r["path"]:r["sha256"] for r in json.loads((root/"source-files.json").read_bytes())}
    helper=json.loads((BUILD/"r8/cuda-runtime-producer-lock.json").read_bytes())["offline_helper"]
    if original.get(helper["path"])!=helper["before_sha256"]:
        raise ValueError("offline helper does not apply to pristine source")
    original[helper["path"]]=helper["after_sha256"]
    ziprow=recipe["dependency_archives"]["cudnn_frontend"]
    zip_path=source_path(ziprow["path"])
    if sha(zip_path)!=ziprow["sha256"]:
        raise ValueError("cuDNN frontend original archive drift")
    frontend={}
    with zipfile.ZipFile(zip_path) as archive:
        for item in archive.infolist():
            if item.is_dir() or stat.S_ISLNK(item.external_attr>>16):
                continue
            prefix="cudnn-frontend-1.27.0/"
            if not item.filename.startswith(prefix):
                raise ValueError("unexpected frontend source root")
            with archive.open(item) as f:
                frontend[item.filename[len(prefix):]]=hashlib.file_digest(f,"sha256").hexdigest()
    for name,digest in cfg["source"]["linux_sources"].items():
        owner, relative=name.split("/",1)
        roster={"source":original,"frontend":frontend}[owner]
        if relative not in roster:
            raise ValueError("projection absent from original source")
        roster[relative]=digest
    return {"ort":match(root/"source",original,root/"source/mirror"),
            "frontend":match(root/"frontend",frontend)}


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--configure",type=Path,required=True)
    p.add_argument("--sha256",required=True)
    a=p.parse_args()
    if sha(a.configure)!=a.sha256:
        raise ValueError("unreviewed configure receipt")
    config=json.loads(a.configure.read_bytes())
    print(json.dumps(verify(Path(config["root"]),config),sort_keys=True))
