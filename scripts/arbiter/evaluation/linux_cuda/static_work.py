# SPDX-License-Identifier: Apache-2.0
"""Count unbuilt GNU archive work in the actual graph; compare against prior native CPU build timings."""
import json
from pathlib import Path
import re
import runpy
import shlex
import subprocess

from checkpoint_ort import BUILD, ROOT, owner
from configure_ort import BASE, CPU, CPU_SHA, sha
from finish_ort import AGGREGATE


def scope():
    ninja, _ = owner()
    aggregate = CPU.with_name("static-onnx-aggregate.py")
    if sha(CPU) != CPU_SHA or sha(aggregate) != AGGREGATE:
        raise ValueError("static closure owner differs")
    cpu = runpy.run_path(str(CPU))
    targets = runpy.run_path(str(aggregate))["runtime_closure"](
        (BUILD/"build.ninja").read_text(),cpu["STATIC_COMPONENTS"],False,cpu["relative_name"])
    raw = subprocess.run([str(ninja), "-C", str(BUILD), "-t", "commands", *targets], check=True,
                         capture_output=True,text=True,timeout=30).stdout
    out = set()
    for line in raw.splitlines():
        words = shlex.split(line)
        if "-o" in words:
            out.add(words[words.index("-o")+1])
    objects = {name for name in out if name.endswith(".o")}
    missing = {name for name in objects if not (BUILD/name).is_file()}
    def key(name):
        return re.sub(r"/home/tyler/airk-builds/arbiter-final-linux-r1/(?:inputs/static-ort|linux-cuda-ort-r4)/",
                      "@ROOT/",name)
    original = BASE/"inputs/static-ort/b/Release/.ninja_log"
    observed = {}
    for line in original.read_text().splitlines():
        if not line.startswith("#"):
            lo, hi, _, name, _ = line.split("\t")
            observed[key(name)] = (int(hi)-int(lo))/1000
    times = [observed[key(name)] for name in missing if key(name) in observed]
    return {"graph_sha256":sha(BUILD/"build.ninja"),"original_cpu_ninja_log":sha(original),
            "archive_targets":len(targets),"all_commands":len(raw.splitlines()),"object_commands":len(objects),
            "existing_objects":len(objects-missing),"remaining_objects":len(missing),"matched_cpu_objects":len(times),
            "sum_cpu_historical_worker_wall_seconds":round(sum(times),3),
            "longest_matched_cpu_object_seconds":max(times,default=None),
            "unmatched_sample":sorted(n for n in missing if key(n) not in observed)[:6],
            "existing_archives":sum((BUILD/t).is_file() for t in targets)}


if __name__ == "__main__":
    print(json.dumps(scope(), sort_keys=True))
