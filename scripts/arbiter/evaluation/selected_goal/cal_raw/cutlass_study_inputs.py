# SPDX-License-Identifier: Apache-2.0
"""Authenticate complete R5 evidence for the unregistered native Compact Read study."""
import argparse
import json
from pathlib import Path
import runpy
import subprocess
import sys

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE / "compact_matching"), str(HERE)]
from assemble import json_pin, settled
from inputs import (AUDIT, CAMPAIGN, SCIENCE, STAGE_SHA, checked, chronology,
                    future_rows, require)
from run import MARKER, verify_result
from phase_reference import digest

GPU = runpy.run_path(str(SCIENCE / "compact_cutlass_cal.py"))
numerical = runpy.run_path(str(SCIENCE / "probe_compact_cutlass.py"))["numerical"]
RECIPE = "7efd4fc72f11de95131c532467803a6a2c82c71cf84ac0bd23cabbcf3662e394"
FOCUS = "94c65d0c598b0b110cd25ddeb1c167ae730430197c0d5e6a59d79ee226c17c64"


def require_full(rows):
    require([row["ordinal"] for row in rows] == list(range(100)) and
            len({row["id"] for row in rows}) == 100, "complete ordered R5 100 required")


def verified_certificate(pin, terminal):
    audit = CAMPAIGN / "cal100-focus1024-launch-r1/cal100-launch-audit.json"
    command = [sys.executable, "-I", "-B", "-S", str(HERE.parent / "cal_certificates/certify.py"),
               "--capture", str(chronology.ROOT), "--active-audit", str(audit),
               "--audit-sha256", AUDIT, "--focus-manifest", FOCUS,
               "--outer-sha256", terminal[0], "--closure-sha256", terminal[1],
               "--mutex-release-sha256", terminal[2]]
    expected = json.loads(subprocess.run(command, check=True, stdout=subprocess.PIPE).stdout)
    require(json_pin(pin) == expected, "original signed source gain or closure changed")
    return expected


def references(pins, role, rows, projections):
    scores = []
    for pin in pins:
        batch = json_pin(pin)
        first, count = batch["first"], batch["count"]
        require(batch["schema"] == "airk.private.compact-protected-cal-cpu-batch/1" and
                batch["complete"] is True and settled(batch["owned"]) and batch["role"] == role and
                first == len(scores) and type(count) is int and 0 < count <= 100-first and
                batch["input"]["sha256"] in projections and
                first+count <= projections[batch["input"]["sha256"]], "wrong reference source or range")
        argv = batch["command"]
        script = HERE / "compact_matching/run.py"
        require(argv[1:5] == ["-I","-B","-S","-X"] and Path(argv[6]) == script and
                argv[7:17] == ["--input",batch["input"]["path"],"--input-sha256",batch["input"]["sha256"],
                               "--first",str(first),"--count",str(count),"--role",role] and
                batch["sources"][str(script)] == digest(script) and
                json.loads(argv[argv.index("--source-set")+1]) == batch["sources"], "different CPU reference worker")
        log = Path(pin["path"]).parent / "worker.log"
        require(log.resolve() == log and log.stat().st_size <= 16 << 20, "foreign or oversized oracle log")
        marked = [line[len(MARKER):] for line in log.read_text("utf-8").splitlines() if line.startswith(MARKER)]
        result = batch["result"]
        require(len(marked) == 1 and json.loads(marked[0]) == result, "CPU result differs from actual worker")
        verify_result(result, rows[first:first+count], "r5_original", role)
        require(result["candidate_graph"]["sha256"] == GPU["GRAPH"] and
                result["candidate_weights"]["sha256"] == GPU["WEIGHTS"] and
                result["protected_stage"]["sha256"] == STAGE_SHA, "different reference model")
        scores.extend(result["rows"])
    require_full(scores)
    return scores


def provider(pin, phase, rows, models):
    job = json_pin(pin)
    require(job["schema"] == "airk.private.compact-cutlass-owned/1" and job["complete"] is True
            and job["mode"] == "cal" and settled(job["owned"]) and job["command"] ==
            [job["image"]["path"], "--exact", GPU["SELECTOR"], "--ignored", "--nocapture"] and
            job["runtime_manifest"]["sha256"] ==
            "9433753580c8a370bbea42119aa67ac81306229bcf6876514213f3e2979083b5",
            "different native GPU issuer, runtime or Job settlement")
    for ref in (job["image"], job["build"], job["runtime_manifest"]):
        path = Path(ref["path"])
        require(path.is_absolute() and path.resolve() == path and digest(path) == ref["sha256"],
                "immutable native image, build or runtime differs")
    limits = job["original_limits"]
    require(limits["seconds"] == 120 and 0 < limits["host_bytes"] <= 32 << 30 and
            limits["ort_arena_bytes"] == 24 << 30, "original native budget changed")
    env = dict(job["cal_inputs"])
    model = models.get(env["AIRK_CUDA_CAL_CHRONOLOGICAL_COMPACT_SHA256"])
    require(model and Path(model["path"]) == Path(env["AIRK_CUDA_CAL_CHRONOLOGICAL_COMPACT"])
            and int(env["AIRK_CUDA_CAL_PHASE"]) == phase and env["AIRK_CUDA_CAL_LAUNCH_AUDIT_SHA256"] == AUDIT,
            "different verified projection, audit or phase")
    journal = Path(pin["path"]).parent / "temp/protected.operations.jsonl"
    require(Path(job["operations"]["path"]) == journal, "different native journal")
    events = checked(job["operations"]).splitlines()
    require(bool(events) == any(row["status"] == "eligible" for row in rows), "missing or unexpected forwards")
    env["AIRK_COMPACT_CUTLASS_EXPIRES_NS"] = str(json.loads(events[0])["expires_monotonic_ns"] if events else 0)
    env["AIRK_COMPACT_CUTLASS_TEST_IMAGE_SHA"] = job["image"]["sha256"]
    result, logs, scratch = GPU["observation"](checked(job["original_stdout"]), journal, rows, env,
                                              job["owned"]["owned_leader_pid"], GPU["TYPED"])
    require(result == job["result"] and logs == job["kernel_records"] and scratch == job["scratch_records"],
            "native raw scores or observed placement changed")
    return result["cal"]["rows"]


def prepare(descriptor):
    full = json_pin(descriptor["full"])
    terminal = full["terminal"]
    require(len(terminal) == 3 and len(full["lifetimes"]) == 2, "original R5 natural closure required")
    rows, _ = future_rows(descriptor["full"])
    require_full(rows)
    pre = verified_certificate(descriptor["certificate"], terminal)
    require([r["id"] for r in rows] == [r["id"] for r in pre["rows"]], "certificate order differs")
    projections, models = {}, {}
    for name, count in (("first56", 56), ("full", 100)):
        pin = descriptor[name]
        receipt = json_pin(pin)
        model = receipt["models"]["compact"]
        projected = json_pin(model)
        audit_reports = json_pin(full["models"]["compact"])["phases"][:(count+7)//8]
        require(len(projected["phases"]) == len(audit_reports) and all(
            x["sha256"] == y["sha256"] and json_pin(x) == json_pin(y)
            for x,y in zip(projected["phases"],audit_reports,strict=True)), "foreign phase evidence")
        require(receipt["plan_sha256"] == chronology.PLAN and receipt["lifetimes"] == full["lifetimes"][:1 if count == 56 else 2]
                and projected["rows"] == rows[:count],
                "foreign prefix or changed compiler projection")
        native = json_pin(receipt["original_compiler_verification"])
        require(native["passed"] is True and settled(native["job"]) and native["NN_invocations"] == 0 and
                native["launch_audit_sha256"] == AUDIT and native["image"]["sha256"] ==
                "b3d20acfea363da2afd1780078e07f46f4c3bcd52da5515aa4ed29c0cc192351"
                and native["inputs"]["compact"]["sha256"] == model["sha256"], "missing original native compilation")
        projections[pin["sha256"]] = count
        models[model["sha256"]] = model
    matching = references(descriptor["protected_matching"], "protected_matching", rows, projections)
    publisher = references(descriptor["publisher_fp32"], "publisher_fp32", rows, projections)
    require(len(descriptor["provider"]) == 13, "all original GPU phase receipts required")
    outputs, comparisons = [], []
    for phase, pin in enumerate(descriptor["provider"]):
        chunk = rows[phase*8:phase*8+8]
        for row, score in zip(chunk, provider(pin, phase, chunk, models), strict=True):
            ordinal = row["ordinal"]
            original = pre["rows"][ordinal]
            raw = dict(score, split="calibration", source_group=original["source_group"])
            if original["request"] is not None:
                raw["roster"] = original["request"]["roster"]
                require(row.get("roster", raw["roster"]) == raw["roster"], "original roster differs")
            if row["status"] == "eligible":
                comparisons.append(dict(ordinal=ordinal, **numerical(score["scores"], matching[ordinal]["scores"],
                                                                    publisher[ordinal]["scores"])))
            outputs.append(raw)
    return {"schema":"airk.private.cutlass-study-verified-input/1","graph":GPU["GRAPH"],"weights":GPU["WEIGHTS"],
            "recipe":RECIPE,"plan":chronology.PLAN,"preflight":pre,"provenance":descriptor,"comparisons":comparisons,
            "raw":{"complete":True,"task_qualified":False,"release_qualified":False,"rows":outputs}}


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--descriptor", required=True)
    p.add_argument("--sha256", required=True)
    p.add_argument("--output", type=Path, required=True)
    args = p.parse_args()
    require(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode, "use python -I -B -S")
    packet = prepare(json_pin({"path":args.descriptor,"sha256":args.sha256}))
    out = args.output
    require(out.resolve() == out and out.parent == CAMPAIGN and out.is_dir() and not any(out.iterdir()),
            "empty checked private output directory required")
    name = "verified-study-input.json"
    chronology.check_outputs(out,[name],created=False)
    with (out/name).open("x",encoding="utf-8") as stream:
        json.dump(packet,stream,allow_nan=False,separators=(",",":"))
    chronology.check_outputs(out,[name],created=True)
    print(digest(out/name))
