# SPDX-License-Identifier: Apache-2.0
"""Seal the generated Ninja includes, executable tools and expanded commands for Linux ORT."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import pwd
import re
import shutil
import stat
import subprocess

from unpack import guarded

BASE = Path("/home/tyler/airk-builds/arbiter-final-linux-r1")
DEFAULT_TARGETS = ("libonnxruntime_providers_shared.so","libonnxruntime_providers_cuda.so","onnxruntime_session","re2")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def file_hash(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f,"sha256").hexdigest()


def snapshot(build, tools, env, targets=DEFAULT_TARGETS, graph=None):
    uid = os.getuid()
    if uid != 1000 or os.geteuid()!=uid or pwd.getpwuid(uid).pw_name!="tyler":
        raise ValueError("registered Linux human required")
    graph = graph or build/"build.ninja"
    root = build.parents[1]
    normalized_root = str(root).encode()
    pending = [graph]
    files = {}
    norm = {}
    while pending:
        path = pending.pop()
        if path in files:
            continue
        if not path.is_relative_to(build) and path != graph:
            raise ValueError("generated include escapes build root")
        guarded(path,uid,stat.S_ISREG)
        if path.stat().st_size > 64 << 20 or len(files)>64:
            raise ValueError("Ninja include capacity")
        data = path.read_bytes()
        index = path.relative_to(build).as_posix() if path.is_relative_to(build) else path.name
        files[path] = (index,digest(data))
        norm[index] = digest(data.replace(normalized_root,b"@BUILD_ROOT@"))
        for line in data.decode().splitlines():
            if not line.startswith(("include ","subninja ")):
                continue
            name = line.split(None,1)[1]
            relative = PurePosixPath(name)
            if not re.fullmatch(r"[a-zA-Z0-9_./-]+",name) or relative.is_absolute() or ".." in relative.parts:
                raise ValueError("unreviewed Ninja include spelling")
            pending.append(build/name)
    if "CMakeFiles/rules.ninja" not in norm:
        raise ValueError("actual CMake command-rule include missing")
    tool_paths = {name:Path(path) for name,path in tools.items()}
    cuda = BASE/"cuda-build-inputs-r1/cuda"
    for name,path in (("nvcc",cuda/"bin/nvcc"),("cicc",cuda/"nvvm/bin/cicc"),
                      ("ptxas",cuda/"bin/ptxas"),("fatbinary",cuda/"bin/fatbinary"),("nvlink",cuda/"bin/nvlink")):
        tool_paths[name] = path
    for name in ("cc1","cc1plus","as","ld","collect2","ar","ranlib"):
        program = subprocess.run([tools["cxx"],"-print-prog-name="+name],env=env,check=True,
                                 capture_output=True,text=True,timeout=5).stdout.strip()
        resolved = program if "/" in program else shutil.which(program,path=env["PATH"])
        if not resolved:
            raise ValueError("underlying compiler tool is unavailable: "+name)
        tool_paths[name] = Path(resolved)
    for name in ("cc","cxx"):
        wrapper = (BASE/"toolchain/bin"/name).read_text()
        actual = re.findall(r"^exec (/[^\s]+)",wrapper,re.M)
        if len(actual)!=1:
            raise ValueError("unexpected configured compiler wrapper")
        tool_paths["driver-"+name]=Path(actual[0])
    sealed_tools = {}
    for name,p in tool_paths.items():
        actual=p.resolve(strict=True)
        if not (actual.is_relative_to(BASE) or actual.is_relative_to("/usr")):
            raise ValueError("tool escapes authenticated execution roots")
        st=actual.stat()
        if not stat.S_ISREG(st.st_mode) or st.st_uid not in (0,uid) or st.st_mode & 0o022:
            raise ValueError("unsafe Linux build tool")
        sealed_tools[name]={"path":str(actual),"bytes":st.st_size,"sha256":file_hash(actual)}
    commands={}
    for target in targets:
        cmd=[tools["ninja"],"-C",str(build),"-f",str(graph),"-t","commands",target]
        raw=subprocess.run(cmd,env=env,check=True,capture_output=True,timeout=30).stdout
        if not 0<len(raw)<=64<<20:
            raise ValueError("expanded Ninja command capacity")
        commands[target]={"count":len(raw.splitlines()),"bytes":len(raw),"sha256":digest(raw),
                          "normalized":digest(raw.replace(normalized_root,b"@BUILD_ROOT@"))}
    return {"files":dict(files.values()),"normalized_files":norm,"tools":sealed_tools,"commands":commands}


if __name__ == "__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--configure",type=Path,required=True)
    p.add_argument("--sha256",required=True)
    opts=p.parse_args()
    if file_hash(opts.configure)!=opts.sha256:
        raise ValueError("unknown Linux CMake receipt")
    cfg=json.loads(opts.configure.read_bytes())
    print(json.dumps(snapshot(Path(cfg["root"])/"b/Release",cfg["tools"],cfg["environment"]),sort_keys=True))
