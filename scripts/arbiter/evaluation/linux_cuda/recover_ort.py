# SPDX-License-Identifier: Apache-2.0
"""Recover the stopped R4 provider after a guest reboot within the original wall-clock ceiling."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import time

import authenticated_build
import build_ort
import checkpoint_ort
from configure_ort import BASE, CAMPAIGN, sha
from verified_inputs import identity

CHECKPOINT = "17bda5d5fbd101f9389f493a6141321753aea3f89c6e1377a622f9bf2b98b02a"


def prefix(path, length):
    with path.open("rb") as stream:
        return hashlib.sha256(stream.read(length)).hexdigest()


def retained():
    identity()
    source = CAMPAIGN / "linux-cuda-r4-checkpoint-custody-r1.json"
    if sha(source) != CHECKPOINT:
        raise ValueError("original retained native checkpoint differs")
    old = json.loads(source.read_bytes())["tree"]["files"]
    build = checkpoint_ort.BUILD
    current = checkpoint_ort.tree()
    rows = current["files"]
    allowed = {".ninja_deps", ".ninja_log"}
    if not old.keys() <= rows.keys() or any(rows[name] != old[name] for name in old.keys() - allowed):
        raise ValueError("earlier retained build sources or compiled files changed")
    for name in allowed:
        if prefix(build / name,old[name]["bytes"]) != old[name]["sha256"]:
            raise ValueError("original Ninja journal prefix differs: " + name)
    fresh = rows.keys() - old.keys()
    log = checkpoint_ort.ROOT / "build-provider-r2.log"
    lines = log.read_text().splitlines()
    if not lines or lines[-1] != "ninja: build stopped: interrupted by user.":
        raise ValueError("original native observer did not record an interrupted build")
    finished = {}
    for line in lines:
        match = re.fullmatch(r"\[(\d+)/(\d+)\] Building (?:CXX|CUDA|C) object (.+\.o)",line)
        if match:
            if int(match[1]) != len(finished) + 1 or int(match[2]) != 392 or match[3] in finished:
                raise ValueError("unexpected previous object completion sequence")
            finished[match[3]] = int(match[1])
    if len(finished) != 323 or set(finished) != fresh:
        raise ValueError("new files differ from actual retained completion log")
    journal = (build / ".ninja_log").read_text().splitlines()
    recorded = {line.split("\t")[3] for line in journal if not line.startswith("#")}
    if not fresh <= recorded:
        raise ValueError("missing completed object in retained Ninja journal")
    exe, argv = checkpoint_ort.owner()
    for workers in (2,4):
        if checkpoint_ort.processes([s.replace("-j2","-j"+str(workers)) for s in argv]):
            raise ValueError("the previous provider compiler is still active")
    cfg = json.loads((CAMPAIGN / "linux-cuda-ort-configure-r4.json").read_bytes())
    raw = subprocess.run([str(exe),"-C",str(build),"-t","commands","libonnxruntime_providers_cuda.so"],
                         check=True,capture_output=True,timeout=30).stdout
    rules = cfg["graph_proof"]["commands"]["libonnxruntime_providers_cuda.so"]
    if hashlib.sha256(raw).hexdigest() != rules["sha256"]:
        raise ValueError("authenticated provider command inventory changed")
    commands = set()
    for line in raw.decode().splitlines():
        args = shlex.split(line)
        if "-o" in args:
            commands.add(args[args.index("-o")+1])
    if not fresh <= commands:
        raise ValueError("retained object has no compile output in the original provider graph")
    birth = int(subprocess.check_output(["/usr/bin/stat","--format=%W",str(checkpoint_ort.ROOT/"build-provider-r1.log")]))
    expires = birth + 10800
    if birth <= 0 or not 0 < expires - time.time() < 10800:
        raise ValueError("original provider window expired")
    return {"checkpoint":CHECKPOINT,"tree_sha256":current["sha256"],"files":current["count"],
            "prior_held_files":len(old),"new_object_files":len(fresh),"previous_phase_log":sha(log),
            "new_objects_sha256":hashlib.sha256(json.dumps({name:rows[name] for name in sorted(fresh)},
                                         separators=(",",":")).encode()).hexdigest(),
            "original_log_birth_utc":birth,"expires_utc":expires}


def main(args):
    custody = retained()
    if args.phase == "inspect":
        return custody
    if not args.custody or sha(args.custody) != args.custody_sha256 or json.loads(args.custody.read_bytes()) != custody:
        raise ValueError("retained native state differs from inspected post-interruption receipt")
    if shutil.disk_usage(BASE).free < 24 << 30:
        raise ValueError("original guest lacks reserved native build space")
    if args.jobs != 2:
        raise ValueError("two low-priority native workers required")
    args.configure = CAMPAIGN / "linux-cuda-ort-configure-r4.json"
    args.sha256 = checkpoint_ort.CONFIGURE
    before = authenticated_build.validate(args)
    args.expires_boottime = time.clock_gettime(time.CLOCK_BOOTTIME) + custody["expires_utc"] - time.time()
    args.target = "provider"
    args.attempt = 3
    result = build_ort.run(args)
    finished = time.time()
    after = authenticated_build.validate(args)
    if before != after or finished > custody["expires_utc"]:
        raise ValueError("recovered provider input or original UTC deadline differs")
    result.update({"authenticated_inputs_before":before,"authenticated_inputs_after":after,"custody":custody,
                   "custody_receipt":args.custody_sha256,"finished_utc":finished,
                   "guest_free_after":shutil.disk_usage(BASE).free})
    return result


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--phase",choices=("inspect","build"),required=True)
    p.add_argument("--custody",type=Path)
    p.add_argument("--custody-sha256")
    p.add_argument("--jobs",type=int,default=2)
    print(json.dumps(main(p.parse_args()),sort_keys=True))
