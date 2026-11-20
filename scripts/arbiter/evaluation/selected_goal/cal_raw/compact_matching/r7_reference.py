# SPDX-License-Identifier: Apache-2.0
"""Run one independent, source-sealed Compact CPU reference for closed R7 rows."""
import argparse
import json
import math
import os
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from inputs import CAMPAIGN, HERE, ORIGINAL, RAW, SCIENCE, STAGE, STAGE_SHA, digest, ref, require, row_key
from panel import reference_identity
from phase_reference import ROOT
from r7_affinity import place_optional_worker, source_paths as placement_sources
from r7_inputs import selected
from run import LIMIT, MARKER, source_set as historical_sources, verify_result
from science import owners

ROLES = ("protected_matching", "publisher_fp32")
SCHEMA = "airk.private.compact-protected-cal-r7-cpu-batch/1"


def source_set():
    sources = historical_sources()
    paths = (
        Path(__file__).resolve(), HERE / "panel.py", HERE / "r7_affinity.py", HERE / "r7_inputs.py",
        RAW / "r7_chronological_inputs.py", RAW / "run_r7_compiler_batch.py",
        SCIENCE / "compact_cutlass_cal.py", SCIENCE / "compact_cutlass_cal_r7.py",
        RAW.parent / "cal_certificates/r6_phased_closure.py",
    )
    return sources | {str(path): digest(path) for path in (*paths, *placement_sources())}


def inputs(args):
    return selected(ref(args.projection, args.projection_sha256), ref(args.compiler, args.compiler_sha256),
                    ref(args.graph, args.graph_sha256), args.first, args.count)


def check_deadline(deadline):
    require(time.monotonic() < deadline, "original R7 CPU reference Job deadline expired")


def checked_science(rows, role, provenance, deadline, cal_pid, terminal, worker_image, sources):
    def check():
        check_deadline(deadline)

    placement = place_optional_worker(cal_pid, 2, deadline=deadline, worker_image=worker_image, terminal=terminal)
    require(len(placement["logical_cpus"]) == 2 and len(set(placement["logical_cpus"])) == 2
            and placement["priority_class"] == 0x4000, "two quiet physical cores and BelowNormal are required")
    protected, base, replay, wider = owners()
    legacy = replay.read_only_bootstrap()
    proof, basis = legacy.read_pinned_json(CAMPAIGN / "compact-fp16-qkv-fp32-wider-reference-r1" / wider.WIDER_LEAF,
                                          base.owner.REFERENCE_SHA, check)
    stage, stage_pin = legacy.read_pinned_json(STAGE, STAGE_SHA, check)
    require(stage["complete"] is True and stage["candidate_graph"]["sha256"] == protected["GRAPH"]
            and stage["graph_proof"]["causal_fp32_mha"] == 28 and stage["graph_proof"]["fp32_projection_count"] == 196,
            "protected candidate stage differs")
    for role_key in ("candidate_graph", "candidate_weights"):
        pin = stage[role_key]
        require(replay.file_pin(Path(pin["path"]), check) == pin, "protected candidate file changed")
    runtime, runtime_pin = legacy.read_pinned_json(replay.RUNTIME, replay.RUNTIME_SHA, check)
    for pin in proof["publisher_files"].values():
        require(replay.file_pin(Path(pin["path"]), check) == pin, "published reference input changed")
    source_pins = [replay.file_pin(Path(path), check) for path in sources]
    require({pin["path"]: pin["sha256"] for pin in source_pins} == sources, "R7 source bytes changed")
    result = {
        "mode": provenance["mode"], "role": role, "provenance": provenance, "sources": source_pins,
        "original_manifest": ORIGINAL, "publisher_files": proof["publisher_files"], "runtime": runtime_pin,
        "reference_basis": basis, "candidate_graph": stage["candidate_graph"], "candidate_weights": stage["candidate_weights"],
        "protected_stage": stage_pin, "matching_recipe_sha256": protected["RECIPE_SHA"], "cpu_placement": placement,
        "candidate_forwards": 0, "qualification": False,
    }
    verify_identity(result, provenance)
    scored = [row | {"input_row_sha256": row_key(row)} for row in rows]
    eligible = [row for row in scored if row["status"] == "eligible"]
    if not eligible:
        return result | {"rows": scored}

    before = set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import torch

    torch.set_num_threads(2)
    torch.set_num_interop_threads(1)
    require(torch.get_num_threads() == 2 and torch.get_num_interop_threads() == 1, "unexpected Torch CPU thread fit")
    half = base.owner.module("../arbiter-fp16-reference.py", legacy.REFERENCES[legacy.REFERENCE_OWNER])
    publisher = base.owner.module("../" + legacy.PUBLISHER_OWNER.removeprefix("scripts/"),
                                  legacy.REFERENCES[legacy.PUBLISHER_OWNER])
    half.freeze_reference_math(torch)
    original, config = publisher.load_authenticated(legacy.PUBLISHER_ROOT, "Qwen/Qwen3-1.7B")
    if role == "protected_matching":
        model = half.HalfReference(torch, config, original.text.state_dict(), False, "cpu")
        del original
        check_protected_attention(protected, torch, check)

    with torch.inference_mode():
        for row in eligible:
            started = time.monotonic()
            check()
            if role == "publisher_fp32":
                scores = base.publisher_scores(torch, original, row["compiled"])
            else:
                scores = protected["score"](half, model, row["compiled"], check)
            check()
            require(len(scores) == len(row["roster"]["cards"]) and all(math.isfinite(value) for value in scores),
                    "incomplete or nonfinite independent R7 scores")
            row.update(scores=scores, seconds=time.monotonic() - started)

    require(not torch.cuda.is_initialized(), "CPU-only R7 reference initialized CUDA")
    pins = dict(runtime["files"])
    for pin in [*proof["reference_sources"], *source_pins]:
        pins[pin["path"]] = pin
    legacy.loaded_runtime(pins, replay.file_pin, check, [sys.modules[name] for name in sys.modules.keys() - before])
    return result | {"rows": scored}


def check_protected_attention(protected, torch, check):
    q = torch.zeros(1, 16, 2, 128)
    k = torch.zeros(1, 8, 2, 128)
    v = torch.arange(8, dtype=torch.float32).view(1, 8, 1, 1).expand(1, 8, 2, 128).clone()
    v[:, :, 1] += 2.
    expected = v.repeat_interleave(2, dim=1)
    expected[:, :, 1] -= 1.
    require(torch.equal(protected["attention"](torch, q, k, v, [1, 1], check), expected),
            "protected causal/grouped-head control differs")


def verify_identity(result, provenance):
    reference_identity(result)
    require(result["provenance"] == provenance and result["qualification"] is False
            and result["candidate_graph"]["sha256"] == provenance["model_sha256"]
            and result["candidate_weights"]["sha256"] == provenance["weight_sha256"]
            and result["matching_recipe_sha256"] == provenance["matching_recipe_sha256"],
            "R7 reference does not bind the same protected identity")


def verified_runtime(deadline=None):
    _, _, replay, wider = owners()
    legacy, _ = wider.load_base()
    if deadline is None:
        deadline = time.monotonic() + legacy.MAX_WALL_SECONDS
    plan, _ = legacy.read_pinned_json(wider.BASE_ARGUMENTS["conversion_plan"],
                                     wider.BASE_ARGUMENTS["conversion_plan_sha256"], lambda: check_deadline(deadline))
    wrapper = Path(plan["wrapper"]["path"])
    if replay.file_pin(wrapper) != plan["wrapper"]:
        wrapper = CAMPAIGN / "compact-conversion-owner-311d13cc" / "arbiter-fp16-execution.py"
    require(digest(wrapper) == plan["wrapper"]["sha256"], "original Windows conversion guard differs")
    conversion = legacy.load_module(wrapper, "compact_cal_r7_cpu_guard")
    token = conversion.windows_owner()
    require(replay.file_pin(Path(sys.executable)) == plan["python"]
            and token["user_sid"] == token["owner_sid"] == plan["human_sid"], "unexpected Python or Windows owner")
    return conversion, legacy, plan, token, deadline


def owned(args, rows, provenance, sources):
    conversion, legacy, plan, token, deadline = verified_runtime()
    helper = ROOT / "scripts/cuda-native-execution/helpers"
    for leaf, sha in conversion.HELPERS.items():
        require(digest(helper / leaf) == sha, "original suspended owner helper differs")
    sys.path.insert(0, str(helper))
    parent = legacy.load_module(helper / "reference_parent_no_site_r2.py", "compact_cal_r7_cpu_parent")
    job = legacy.load_module(helper / "reference_owned.py", "compact_cal_r7_cpu_job")
    out = args.output
    require(out.is_absolute() and out == out.resolve() and out.parent == CAMPAIGN and not out.exists(),
            "fresh original campaign output required")
    cache = out / "temp"
    for path in (out, cache):
        conversion.guard(plan, path, deadline=deadline)
        path.mkdir()
        conversion.guard(plan, path, True, deadline=deadline)
    log, execution = out / "worker.log", out / "execution.json"
    for path in (log, execution):
        conversion.guard(plan, path, deadline=deadline)
    excluded = ("AIRK_", "ORT_", "PYTHON", "CUDA_", "CUDNN_", "TORCH_", "HF_", "TRITON_")
    env = {name: value for name, value in os.environ.items() if not name.upper().startswith(excluded)}
    env.update(CUDA_VISIBLE_DEVICES="", HF_HUB_OFFLINE="1", TRANSFORMERS_OFFLINE="1", TOKENIZERS_PARALLELISM="false",
               TORCH_HOME=str(cache), TORCHINDUCTOR_CACHE_DIR=str(cache), TRITON_CACHE_DIR=str(cache),
               HF_HOME=str(cache / "hf"), XDG_CACHE_HOME=str(cache), TEMP=str(cache), TMP=str(cache), TMPDIR=str(cache))
    argv = [sys.executable, "-I", "-B", "-S", "-X", "pycache_prefix=" + plan["cache_prefix"], str(Path(__file__).resolve()),
            "--worker-deadline", repr(deadline), "--source-set", json.dumps(sources, sort_keys=True, separators=(",", ":")),
            "--first", str(args.first), "--count", str(args.count), "--role", args.role]
    for name in ("projection", "compiler", "graph"):
        argv.extend(["--" + name, provenance[name]["path"], "--" + name + "-sha256", provenance[name]["sha256"]])
    if args.cal_pid is not None:
        argv.extend(["--cal-pid", str(args.cal_pid)])
    else:
        argv.extend(["--terminal", *args.terminal])

    def suspended(api, handle, info, child_deadline):
        evidence = parent.suspended_authentication(api, info, deadline, child_deadline,
                                                   conversion.SID, plan["python"], job.image_proof)
        require(conversion.windows_owner(process=info.process) == token and job.job_snapshot(api, handle) == ([info.pid], 1, 1),
                "unverified suspended R7 worker or Job")
        return evidence

    with open(os.devnull, "rb") as stdin, log.open("xb") as stream:
        conversion.guard(plan, log, True, deadline=deadline)
        observation = job.run_windows_owned_training(argv, ROOT, stream, deadline - time.monotonic(), env, stdin, LIMIT,
                                                     before_resume=suspended, absolute_deadline=deadline, cleanup_seconds=5)
    for path in [out, cache, log, *cache.rglob("*")]:
        conversion.guard(plan, path, True, deadline=time.monotonic() + 10)
    require(log.stat().st_size <= 16 << 20, "original R7 CPU reference log limit exceeded")
    lines = [line[len(MARKER):] for line in log.read_text("utf-8").splitlines() if line.startswith(MARKER)]
    error = None
    result = None
    complete = False
    try:
        require(parent.settled(observation) and len(lines) == 1 and sources == source_set(),
                "R7 CPU reference owner, result line or source custody differs")
        measured = json.loads(lines[0])
        verify_result(measured, rows, provenance["mode"], args.role)
        verify_identity(measured, provenance)
        require(inputs(args) == (rows, provenance), "R7 source inputs changed after the worker")
        result = measured
        complete = True
    except (ValueError, KeyError, OSError) as exception:
        error = str(exception)
    receipt = {
        "schema": SCHEMA, "complete": complete, "first": args.first, "count": args.count, "role": args.role,
        "input": provenance, "sources": sources, "command": argv, "owned": observation, "log_sha256": digest(log),
        "error": error, "result": result,
    }
    with execution.open("x", encoding="utf-8") as stream:
        json.dump(receipt, stream, allow_nan=False)
    conversion.guard(plan, execution, True, deadline=time.monotonic() + 10)
    require(complete, "incomplete original R7 CPU reference; retain its Job and log")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    run_mode = parser.add_mutually_exclusive_group(required=True)
    run_mode.add_argument("--check-only", action="store_true")
    run_mode.add_argument("--output", type=Path)
    run_mode.add_argument("--worker-deadline", type=float)
    for name in ("projection", "compiler", "graph"):
        parser.add_argument("--" + name, type=Path, required=True)
        parser.add_argument("--" + name + "-sha256", required=True)
    parser.add_argument("--first", type=int, required=True)
    parser.add_argument("--count", type=int, required=True)
    parser.add_argument("--role", choices=ROLES, required=True)
    affinity = parser.add_mutually_exclusive_group()
    affinity.add_argument("--cal-pid", type=int)
    affinity.add_argument("--terminal", nargs=3)
    parser.add_argument("--source-set")
    args = parser.parse_args()
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "use the original checked Windows Python -I -B -S")
    if not args.check_only:
        require(args.cal_pid is not None or args.terminal is not None, "explicit live CAL PID or original terminal custody required")
    rows, provenance = inputs(args)
    sources = source_set()
    if args.check_only:
        print(json.dumps({"input": provenance, "role": args.role, "forwards": 0,
                          "rows": [{"ordinal": row["ordinal"], "status": row["status"],
                                    "input_row_sha256": row_key(row)} for row in rows]}, allow_nan=False))
    elif args.output is not None:
        owned(args, rows, provenance, sources)
    else:
        require(0 < args.worker_deadline - time.monotonic() <= 600 and sources == json.loads(args.source_set),
                "foreign parent source proof or worker deadline")
        _, _, plan, _, _ = verified_runtime(args.worker_deadline)
        terminal = dict(zip(("outer", "fixture", "mutex"), args.terminal, strict=True)) if args.terminal else None
        result = checked_science(rows, args.role, provenance, args.worker_deadline,
                                 args.cal_pid, terminal, plan["python"], sources)
        verify_result(result, rows, provenance["mode"], args.role)
        print(MARKER + json.dumps(result, separators=(",", ":"), allow_nan=False))


if __name__ == "__main__":
    main()
