# SPDX-License-Identifier: Apache-2.0
"""Compile an sm_86 half-precision object in WSL; never execute it."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import pwd
import runpy
import stat

from unpack import guarded


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def probe(inputs, out, uid):
    if uid == 0 or uid != os.getuid() or uid != os.geteuid() or pwd.getpwuid(uid).pw_name != "tyler":
        raise ValueError("expected registered human UID")
    home = Path(pwd.getpwuid(uid).pw_dir)
    if (out.parent != inputs.parent or not out.is_relative_to(home) or os.path.lexists(out)
            or inputs.resolve(strict=True) != inputs):
        raise ValueError("canonical fresh human-owned sibling output required")
    for p in (inputs, *inputs.parents):
        guarded(p, uid, stat.S_ISDIR)
        if p == home:
            break
    script = Path(__file__).resolve()
    service = script.parents[3] / "provision-static-onnx-runtime.py"
    if digest(service) != "a1772271cfc3e585de903a7e80021b54f97da823a485e48621a60dcee564c044":
        raise ValueError("shared process owner differs")
    runner = runpy.run_path(str(service))["run_owned"]
    compiler = inputs / "cuda/bin/nvcc"
    host = inputs.parent / "toolchain/bin/cxx"
    for program in (compiler, host):
        guarded(program, uid, stat.S_ISREG)
    os.umask(0o077)
    out.mkdir(0o700)
    guarded(out, uid, stat.S_ISDIR)
    tmp = out / "tmp"
    tmp.mkdir(0o700)
    guarded(tmp, uid, stat.S_ISDIR)
    source = script.with_name("compiler_probe.cu")
    argv = [compiler, "-ccbin", host, "-arch=sm_86", "-std=c++17", "--threads", "1",
            "--keep", "--keep-dir", tmp, "-I" + str(inputs / "cudnn/include"), "-c", source, "-o", out / "probe.o"]
    env = {"LANG": "C", "LC_ALL": "C", "PATH": str(compiler.parent) + ":/usr/bin:/bin", "HOME": str(home),
           "TMP": str(tmp), "TEMP": str(tmp), "TMPDIR": str(tmp)}
    result = runner(argv, out, out / "compiler.log", 120, env)
    outputs = {}
    for path in sorted(out.rglob("*")):
        guarded(path, uid, lambda mode: stat.S_ISDIR(mode) or stat.S_ISREG(mode))
        if path.is_file():
            outputs[str(path.relative_to(out))] = {"sha256": digest(path), "bytes": path.stat().st_size}
    ok = (result["exit_code"] == 0 and result["owned_descendants_settled"] and "probe.o" in outputs)
    return {"schema": "airk.private.wsl-cuda-compiler-probe/1", "source_sha256": digest(source),
            "nvcc_sha256": digest(compiler), "host_wrapper_sha256": digest(host), "result": result,
            "output_root": str(out), "outputs": outputs, "compiled": ok, "gpu_initialized": False}


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--inputs", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--uid", required=True, type=int)
    args = p.parse_args()
    print(json.dumps(probe(args.inputs, args.output, args.uid), sort_keys=True))
