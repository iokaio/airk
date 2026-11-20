# SPDX-License-Identifier: Apache-2.0
"""Finish a same-build Linux CUDA ELF pair and single static archive from reviewed inputs."""
import argparse
import json
import os
from pathlib import Path
import pwd
import runpy
import stat
import types

from configure_ort import BASE, CAMPAIGN, CPU, CPU_SHA, audit, sha
from inventory import source_record
from unpack import guarded

CFG = "e579de0a21f10459506187c92cd3eb3c93be5b47d10e03619a07f02419f2edf8"
BOOT = "ab8c2756f10cab33a338f8c9960b3673222eb5ca2561aa0cd0143fc494a7ae69"
AGGREGATE = "e13b6ce08b35682e0b4bc68bff83e41165b0d7a94bca5a9ecde530b6577dc233"
TARGET = "x86_64-unknown-linux-gnu"
PROVIDER = "libonnxruntime_providers_cuda.so"
SHARED = "libonnxruntime_providers_shared.so"


def checked_receipt(name, digest):
    path = CAMPAIGN / name
    if sha(path) != digest:
        raise ValueError("receipt changed: " + name)
    return json.loads(path.read_bytes())


def run(options):
    uid = os.getuid()
    if uid != 1000 or os.geteuid() != uid or pwd.getpwuid(uid).pw_name != "tyler":
        raise ValueError("registered non-root human required")
    os.umask(0o077)
    aggregate = CPU.with_name("static-onnx-aggregate.py")
    if sha(CPU) != CPU_SHA or sha(aggregate) != AGGREGATE:
        raise ValueError("static ORT owner differs")
    cpu = types.SimpleNamespace(**runpy.run_path(str(CPU)))
    archive = types.SimpleNamespace(**runpy.run_path(str(aggregate)))
    cfg = checked_receipt("linux-cuda-ort-configure-r4.json", CFG)
    boot = checked_receipt("linux-cuda-bootstrap-compile-r2.json", BOOT)
    source = Path(cfg["root"])
    build = source / "b/Release"
    if source != BASE / "linux-cuda-ort-r4":
        raise ValueError("unexpected ORT output root")
    for p in (BASE.parents[1], BASE.parent, BASE, source, build.parent, build):
        guarded(p, uid, stat.S_ISDIR)
    env = dict(cfg["environment"], CCACHE_DISABLE="1", CUDA_CACHE_DISABLE="1")
    graph = build / "build.ninja"
    if not boot["built"] or sha(graph) != cfg["ninja_sha256"]:
        raise ValueError("source or graph identity changed")
    if (boot["original_env"] != cfg["source"]["unprotected_env_sha256"] or
        boot["projected_env"] != cfg["source"]["linux_sources"]["source/onnxruntime/core/platform/posix/env.cc"] or
        boot["bootstrap"] != cfg["source"]["bootstrap_sha256"]):
        raise ValueError("configured Env differs from reviewed bootstrap")
    def native(stage, digest):
        attempt = options.provider_attempt if stage == "provider" else 1
        record = checked_receipt(f"linux-cuda-ort-r4-{stage}-r{attempt}.json", digest)
        if not record["built"] or not record["rules_unchanged"] or record["configure"] != CFG:
            raise ValueError("incomplete same-build native stage: " + stage)
        if stage != "shared" and record["authenticated_inputs_before"] != record["authenticated_inputs_after"]:
            raise ValueError("native compile inputs changed")
        return record
    if options.phase == "provider":
        result = {"success": True, "configure": CFG, "graph": sha(graph),
                  "shared_receipt": options.shared_receipt, "provider_receipt": options.provider_receipt}
        for stage, digest, name in (("shared", options.shared_receipt, SHARED),
                                    ("provider", options.provider_receipt, PROVIDER)):
            record = native(stage, digest)
            guarded(build / name, uid, stat.S_ISREG)
            image = source_record(build / name)
            if image != record["artifact"] or image["dynamic_tags"].get("SONAME") != [name]:
                raise ValueError("completed ELF changed: " + name)
            result[stage] = image
        return result
    record = native("core", options.core_receipt)
    targets = archive.runtime_closure(graph.read_text(), cpu.STATIC_COMPONENTS, False, cpu.relative_name)
    out = source / "aggregate-evidence"
    if os.path.lexists(out):
        raise ValueError("fresh aggregate evidence directory required")
    out.mkdir(0o700)
    audit(out,uid)
    rows = []
    for name in targets:
        path = build / name
        guarded(path, uid, stat.S_ISREG)
        with path.open("rb") as src:
            if src.read(8) != b"!<arch>\n":
                raise ValueError("missing actual component archive")
        rows.append({"path": path.relative_to(source).as_posix(), "sha256": sha(path), "bytes": path.stat().st_size})
    if sum(row["bytes"] for row in rows) > 2 << 30:
        raise ValueError("static input archive size bound")
    # The canonical GNU archive owner uses the actual selected component paths.
    # Its response destination is under source/aggregate, reserved for this phase.
    dest = source / "aggregate"
    if os.path.lexists(dest):
        raise ValueError("fresh aggregate output required")
    dest.mkdir(0o700)
    call = archive.invocation(source, TARGET, {}, {"inputs": rows}, cpu)
    call["response"].write_text(call["text"])
    audit(dest, uid)
    packed = cpu.run_owned(call["argv"], source, out/"aggregate.log", 300, env)
    audit(dest, uid)
    if not cpu.successful(packed):
        return {"success": False, "aggregate_command":packed}
    file = archive.output_row(source,TARGET,cpu)
    symbol_log = out / "symbols.log"
    symbol_check = cpu.run_owned(["/usr/bin/nm","-g","--defined-only",str(source/file["path"])],
                                 source, symbol_log, 120, env)
    audit(out,uid)
    if not cpu.successful(symbol_check):
        return {"success":False,"aggregate_command":packed,"symbol_command":symbol_check}
    names = ["airk_ort_bootstrap_linux_cuda_v1", "OrtGetApiBase", "airk_ort_collect_final_placement_v1",
             "airk_ort_collect_final_graph_metadata_v1"]
    counts = dict.fromkeys(names,0)
    with symbol_log.open(encoding="utf-8", errors="replace") as lines:
        for row in lines:
            fields = row.split()
            if fields and fields[-1] in counts:
                counts[fields[-1]] += 1
    if any(sha(source / row["path"]) != row["sha256"] for row in rows):
        raise ValueError("component archive changed during aggregation")
    if any(value != 1 for value in counts.values()):
        raise ValueError("static archive did not preserve exact same-image entry points")
    return {"success":True, "target":TARGET, "build_root":str(source), "configure":CFG, "graph":sha(graph),
            "configure_input":{"path":str(CAMPAIGN/"linux-cuda-ort-configure-r4.json"),"sha256":CFG},
            "component_build_input":{"path":str(CAMPAIGN/"linux-cuda-ort-r4-core-r1.json"),"sha256":options.core_receipt},
            "core_receipt":options.core_receipt,"bootstrap":BOOT,"inputs":rows,
            "static":file,"symbols":names,"command":record["command"],"aggregate_command":packed,"symbol_command":symbol_check}


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--phase", choices=["provider","core"], required=True)
    p.add_argument("--shared-receipt")
    p.add_argument("--core-receipt")
    p.add_argument("--provider-receipt")
    p.add_argument("--provider-attempt", type=int, choices=(1, 2, 3), default=1)
    print(json.dumps(run(p.parse_args()), sort_keys=True))
