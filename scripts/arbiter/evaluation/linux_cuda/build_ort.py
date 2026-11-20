# SPDX-License-Identifier: Apache-2.0
"""Compile frozen Linux CUDA targets under an owned offline Ninja process."""
import argparse
import json
import os
from pathlib import Path
import pwd
import resource
import runpy
import stat
import time

from configure_ort import BASE, CPU, CPU_SHA, audit, sha
from graph_proof import snapshot
from inventory import source_record
from unpack import guarded


def available_bytes():
    for line in Path("/proc/meminfo").read_text().splitlines():
        if line.startswith("MemAvailable:"):
            return int(line.split()[1]) * 1024
    raise ValueError("guest memory availability unavailable")


def run(options):
    uid = os.getuid()
    if (uid,os.geteuid(),os.getgid(),pwd.getpwuid(uid).pw_name) != (1000,1000,1000,"tyler"):
        raise ValueError("registered non-root Linux human required")
    if sha(CPU)!=CPU_SHA or sha(options.configure)!=options.sha256:
        raise ValueError("unreviewed configure receipt or runner")
    cfg=json.loads(options.configure.read_bytes())
    if not cfg["configured"] or cfg["schema"]!="airk.private.wsl-cuda-ort-configure/2":
        raise ValueError("configure with authenticated included Ninja command rules required")
    root=Path(cfg["root"])
    if root.parent!=BASE or not 1<=options.attempt<=99:
        raise ValueError("isolated campaign root and fresh attempt required")
    os.umask(0o077)
    build=root/"b/Release"
    for path in (BASE.parents[1],BASE.parent,BASE,root,build.parent,build):
        guarded(path,uid,stat.S_ISDIR)
    audit(root,uid)
    env=dict(cfg["environment"],CCACHE_DISABLE="1",CUDA_CACHE_DISABLE="1",
             CMAKE_BUILD_PARALLEL_LEVEL=str(options.jobs))
    tools=cfg["tools"]
    if snapshot(build,tools,cfg["environment"])!=cfg["graph_proof"]:
        raise ValueError("changed compiler, generated rule, or configured dependencies")
    def sources():
        for rel,digest in cfg["source"]["linux_sources"].items():
            if sha(root/rel)!=digest:
                raise ValueError("changed projected source: "+rel)
    sources()
    cpu=runpy.run_path(str(CPU))
    if options.target == "core":
        archive = CPU.with_name("static-onnx-aggregate.py")
        if sha(archive) != "e13b6ce08b35682e0b4bc68bff83e41165b0d7a94bca5a9ecde530b6577dc233":
            raise ValueError("unreviewed runtime archive selector")
        closure = runpy.run_path(str(archive))["runtime_closure"]
        targets = closure((build / "build.ninja").read_text(), cpu["STATIC_COMPONENTS"], False, cpu["relative_name"])
    else:
        targets = [{"shared":"libonnxruntime_providers_shared.so","provider":"libonnxruntime_providers_cuda.so"}[options.target]]
    before=snapshot(build,tools,env,targets)
    guest_before=available_bytes()
    if guest_before < (12*options.jobs+8) << 30:
        raise ValueError("insufficient measured guest headroom for selected workers")
    command=["/usr/bin/nice","-n","10","/usr/bin/prlimit","--as=12884901888","--",tools["ninja"],
             "-C",str(build),"-j"+str(options.jobs),*targets]
    log=root/("build-"+options.target+"-r"+str(options.attempt)+".log")
    r0=resource.getrusage(resource.RUSAGE_CHILDREN)
    timeout = 300 if options.target == "shared" else 10800
    expires = getattr(options, "expires_boottime", None)
    if expires is not None:
        timeout = min(timeout, expires - time.clock_gettime(time.CLOCK_BOOTTIME))
        if timeout <= 0:
            raise ValueError("original native build deadline expired")
    result=cpu["run_owned"](command,root,log,timeout,env)
    r1=resource.getrusage(resource.RUSAGE_CHILDREN)
    after=snapshot(build,tools,env,targets)
    stable=before==after and snapshot(build,tools,cfg["environment"])==cfg["graph_proof"]
    sources()
    audit(root,uid)
    binary=None
    if options.target!="core" and (build/targets[0]).is_file():
        binary=source_record(build/targets[0])
    elf_ok = (binary is not None and binary["dynamic_tags"].get("SONAME")==targets and
              not any(k in binary["dynamic_tags"] for k in ("RPATH","RUNPATH")))
    return {"schema":"airk.private.wsl-cuda-ort-build/2","configure":sha(options.configure),
            "rules_unchanged":stable,"before":before,"after":after,"targets":targets,"command":result,
            "log_sha256":sha(log),"observer_seconds":timeout,"artifact":binary,"jobs":options.jobs,"nvcc_threads":1,
            "guest_available_before":guest_before,"guest_available_after":available_bytes(),
            "child_cpu_user_seconds":r1.ru_utime-r0.ru_utime,"child_cpu_system_seconds":r1.ru_stime-r0.ru_stime,
            "largest_waited_child_maxrss_kib":r1.ru_maxrss,
            "built":stable and cpu["successful"](result) and (options.target=="core" or elf_ok)}


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--configure",type=Path,required=True)
    parser.add_argument("--sha256",required=True)
    parser.add_argument("--target",required=True,choices=("shared","provider","core"))
    parser.add_argument("--jobs",type=int,choices=(2,4),default=2)
    parser.add_argument("--attempt",type=int,default=1)
    print(json.dumps(run(parser.parse_args()),sort_keys=True))
