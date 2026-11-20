# SPDX-License-Identifier: Apache-2.0
"""Compile the protected Linux ORT host bootstrap using its exact configured translation-unit flags."""
import argparse
import json
import os
from pathlib import Path
import pwd
import runpy
import shlex
import stat
import subprocess

from bootstrap_projection import project_env, link_argv
from graph_proof import snapshot
from configure_ort import BASE, CAMPAIGN, CPU, CPU_SHA, HERE, audit, sha
from unpack import guarded


def run(root):
    uid = os.getuid()
    if uid != 1000 or os.geteuid() != uid or pwd.getpwuid(uid).pw_name != "tyler":
        raise ValueError("registered non-root Linux human required")
    if os.path.lexists(root) or root.parent != BASE or sha(CPU) != CPU_SHA:
        raise ValueError("fresh bootstrap output and pinned owner required")
    os.umask(0o077)
    for p in (BASE.parents[1], BASE.parent, BASE):
        guarded(p, uid, stat.S_ISDIR)
    cfgpath = CAMPAIGN / "linux-cuda-ort-configure-r3.json"
    if sha(cfgpath) != "701cb486dec2761bd1f81371926f2f187bfc6943814f02d677b9f9bd95a6bca2":
        raise ValueError("original configure receipt differs")
    cfg = json.loads(cfgpath.read_bytes())
    original = Path(cfg["root"])
    build = original / "b/Release"
    ninja_file = build / "build.ninja"
    if not cfg["configured"] or sha(ninja_file) != cfg["ninja_sha256"]:
        raise ValueError("configured graph changed")
    proofpath = CAMPAIGN/"linux-cuda-graph-proof-r1.json"
    if sha(proofpath) != "5936a42969648b475ea8bf19fab2fe0f3b41452f606f9c2a0e973810e29a00f1":
        raise ValueError("unreviewed ORT rules/command/tool census")
    expected = json.loads(proofpath.read_bytes())
    if snapshot(build,cfg["tools"],cfg["environment"]) != expected:
        raise ValueError("original ORT compile rules, commands or tools changed")
    root.mkdir(0o700)
    rel = "source/onnxruntime/core/platform/posix/env.cc"
    source = original / rel
    if sha(source) != cfg["source"]["linux_sources"][rel]:
        raise ValueError("configured Env source changed")
    projected = root / "env.cc"
    projected.write_bytes(source.read_bytes())
    bootstrap = HERE / "ort_bootstrap.inc"
    project_env(projected, bootstrap.read_bytes().replace(b"\r\n", b"\n"))
    ninja = cfg["tools"]["ninja"]
    def command(target):
        result = subprocess.run([ninja, "-C", str(build), "-t", "commands", "-s", target],
                                check=True, capture_output=True, timeout=10)
        if len(result.stdout) > 1 << 20 or len(result.stdout.splitlines()) != 1:
            raise ValueError("expected one bounded tool command")
        return result.stdout.decode().strip()
    target = "CMakeFiles/onnxruntime_common.dir/" + str(source).lstrip("/") + ".o"
    before = snapshot(build,cfg["tools"],cfg["environment"],(target,))
    argv = shlex.split(command(target))
    if argv[0] != cfg["tools"]["cxx"] or argv[argv.index("-c") + 1] != str(source):
        raise ValueError("configured compile source/tool differs")
    for flag, val in (("-o", root/"env.cc.o"), ("-MF", root/"env.cc.d"), ("-MT", root/"env.cc.o"), ("-c", projected)):
        if argv.count(flag) != 1:
            raise ValueError("duplicate or missing translation unit argument")
        argv[argv.index(flag)+1] = str(val)
    provider = "libonnxruntime_providers_cuda.so"
    link = link_argv(command(provider), build, root/provider, Path(cfg["tools"]["cxx"]))
    cpu = runpy.run_path(str(CPU))
    result = cpu["run_owned"](["/usr/bin/nice","-n","10",*argv], build, root/"compile.log", 120, cfg["environment"])
    audit(root, uid)
    stable = snapshot(build,cfg["tools"],cfg["environment"],(target,)) == before and snapshot(build,cfg["tools"],cfg["environment"]) == expected
    symbols = []
    if cpu["successful"](result):
        listing = subprocess.run(["/usr/bin/nm", "-g", "--defined-only", str(root/"env.cc.o")],
                                 check=True, capture_output=True, text=True, timeout=10)
        symbols = [row.split()[-1] for row in listing.stdout.splitlines() if row.endswith(" airk_ort_bootstrap_linux_cuda_v1")]
    return {"rules_proof": sha(proofpath), "rules_unchanged": stable, "compile_graph": before, "configure": sha(cfgpath), "graph": sha(ninja_file), "original_env": sha(source),
            "bootstrap": sha(bootstrap), "projected_env": sha(projected), "command": result,
            "projected_link_soname": [v for v in link if "-soname" in v], "symbols": symbols,
            "object": sha(root/"env.cc.o") if (root/"env.cc.o").is_file() else None,
            "built": stable and cpu["successful"](result) and symbols == ["airk_ort_bootstrap_linux_cuda_v1"]}


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--output", type=Path, required=True)
    print(json.dumps(run(p.parse_args().output), sort_keys=True))
