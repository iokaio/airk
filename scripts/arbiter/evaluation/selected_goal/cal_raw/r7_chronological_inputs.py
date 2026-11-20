# SPDX-License-Identifier: Apache-2.0
"""Project only authenticated, chronologically closed R7 CAL observations."""
import argparse
from collections import Counter
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
# Import the byte-pinned R5 arithmetic and Windows output guard without editing either.
import r5_chronological_inputs as original
from certify import real_scope
from inspect_closed_lifetime import phase_report
from r6_epoch import ORIGINAL, ORIGINAL_SHA, R7_ROOT, preflight
from r6_group import TWO_GROUPS
from r6_phased_closure import phased_events
from reference_inputs import digest, load_inputs, read, reference, require

AUDIT = "aa2dea22544689e785426e03d4fa4f657cfda6d5dbc86b682a8594be7bb680e1"
SERVICE = "77e70341e1da84688c8b16bc992e25b3afed32d2d3e07639a6a5c69176eb0dfa"
ARITHMETIC = "6da3b982fb8ef837b5e67b2312de6caf3b54dd72859e53abeced6f2a2d0de40c"
SCHEMA = "airk.private.cal100-r7-chronological-projection/1"


def check_scoped_evidence(evidence, binding, audit):
    status, answer, records, probes = (evidence[key] for key in ("scope", "answer", "captured", "probes"))
    real_scope(status, binding)
    if answer["refused"]:
        require(not probes, "original undelivered Answer was probed")
        return
    body = answer["body"]
    real_scope(body, binding)
    for probe in probes:
        result = probe["result"]
        if not result["refused"]:
            real_scope(result["body"], binding)
            require(result["body"]["publication"] == binding.publication, "original probe publication differs")
    if records:
        record, = records
        require(record["schema"] == "airk.private-selected-Context-native-admission-observation/1" and
                record["actual_Focus_profile"] == audit["profiles"]["arbiter-focus-qwen3-reranker-06b"] and
                record["MiniLM_pipeline"] == audit["pipeline"] and
                [probe["probe"] for probe in probes] == record["verified_probe_requests"],
                "original R7 observer or associated probe roster differs")
        binding.scope(record["request"]["scope"])


def reports_for(plan_ref, plan, lifetimes, observations):
    reports = []
    count = 0
    for group, lifetime_ref in enumerate(lifetimes):
        directory = R7_ROOT / f"lifetime-{group:02}"
        work = directory / "fixture"
        lifetime = read(lifetime_ref, directory / "lifetime-closure.json")
        phases = TWO_GROUPS[group]
        require(lifetime["plan"] == plan_ref and lifetime["phases"] == list(phases) and
                len(lifetime["phase_closures"]) == len(phases), "different native phase custody")
        for phase, phase_pin in zip(phases, lifetime["phase_closures"], strict=True):
            ids = plan["phase_ids"][phase]
            observed = observations[count:count+len(ids)]
            require([row["id"] for row in observed] == ids, "changed or incomplete R7 question order")
            report = phase_report(directory, work, phase, phase_pin, ids,
                                  [(row["id"], len(row["probes"]), row["pipeline_searches"]) for row in observed],
                                  lifetime_ref["sha256"], plan_ref["sha256"], AUDIT)
            reports.append(report)
            count += len(ids)
    require(count == len(observations), "incorrect R7 observation census")
    return reports


def batches(plan_ref, plan, reports, observations, phases):
    require(digest(Path(original.__file__).read_bytes()) == ARITHMETIC, "original compiler projection arithmetic changed")
    require(len(reports) == len(phases) and len(phases) in (7, 13) and
            [r["phase"] for r in reports] == list(range(len(phases))), "incomplete or reordered R7 phases")
    rows_by_model = {model: [] for model in original.MODELS}
    count = 0
    for report in reports:
        expected = plan["phase_ids"][report["phase"]]
        require(report["plan_sha256"] == plan_ref["sha256"] and report["input_audit_sha256"] == AUDIT and
                report["certificate_count"] == 0 and [r["question_id"] for r in report["rows"]] == expected,
                "historical or substituted R7 phase")
        end = count + len(expected)
        for case, evidence in zip(report["rows"], observations[count:end], strict=True):
            require(case["ordinal"] == count, "incorrect R7 ordinal")
            for model, rows in rows_by_model.items():
                rows.append(original.model_row(case, evidence, model))
            count += 1
    require(count == len(observations) == min(8*len(phases), 100) and
            len({evidence["id"] for evidence in observations}) == count, "incomplete R7 projection")
    result = {}
    for model, rows in rows_by_model.items():
        model_id, manifest = original.MODELS[model]
        result[model] = {"model": model_id, "original_manifest": manifest, "launch_audit_sha256": AUDIT,
                         "phase_plan_sha256": plan_ref["sha256"], "phases": phases,
                         "original_requests": len(rows), "statuses": dict(Counter(r["status"] for r in rows)),
                         "first_eligible_ordinal": next((r["ordinal"] for r in rows if r["status"] == "eligible"), None),
                         "NN_invocations": 0, "rows": rows}
    return result


def check_capture_ownership(count, terminal):
    names = ["lifetime-plan.json", "predecessor-control.json", "control-mutex-acquired.json"]
    if terminal:
        names.extend(("outer-owned.json", "fixture-closure.json", "control-mutex-released.json"))
    trees = [R7_ROOT/"candidate-sources", *(R7_ROOT/f"lifetime-{i:02}" for i in range(count))]
    while trees:
        path = trees.pop()
        require(not path.is_symlink() and not path.is_junction() and (path.is_file() or path.is_dir()),
                "ambiguous original R7 output")
        names.append(path.relative_to(R7_ROOT).as_posix())
        if path.is_dir():
            trees.extend(path.iterdir())
    for start in range(0, len(names), 64):
        original.check_outputs(R7_ROOT, names[start:start+64], created=True)


def collect(plan_ref, lifetime_shas, terminal):
    require(len(lifetime_shas) == (2 if terminal else 1), "one closed R7 lifetime or complete R7 terminal required")
    lifetimes = [reference(R7_ROOT/f"lifetime-{i:02}/lifetime-closure.json", sha)
                 for i, sha in enumerate(lifetime_shas)]
    check_capture_ownership(len(lifetimes), terminal)
    plan = read(plan_ref, R7_ROOT / "lifetime-plan.json")
    audit_ref = plan["input_audit"]
    require(audit_ref["sha256"] == AUDIT, "different R7 launch audit")
    context, audit = preflight(R7_ROOT, plan_ref, audit_ref, SERVICE)
    focus = audit["profiles"]["arbiter-focus-qwen3-reranker-06b"]
    _, ids, tasks, _, _, _, sources, index_root = load_inputs(reference(ORIGINAL, ORIGINAL_SHA), focus)
    require(read(audit["census"])["question_ids"] == ids, "original R7 question census changed")
    if terminal:
        outer, whole, release = terminal
        closed = read(reference(R7_ROOT/"fixture-closure.json", whole))
        require(closed["lifetimes"] == lifetimes, "R7 terminal names different native lifetimes")
        events = phased_events(R7_ROOT, outer, whole, audit, AUDIT, ids, sources, index_root, tasks,
                               release_sha=release, r6=context)
    else:
        from r6_phased_closure import prefix_events
        check_live_owner(plan)
        events = prefix_events(R7_ROOT, lifetimes[0], audit, AUDIT, ids, sources, index_root, tasks, r6=context)
    observations = []
    for evidence, binding in events:
        check_scoped_evidence(evidence, binding, audit)
        observations.append(evidence)
    require([row["id"] for row in observations] == ids[:100 if terminal else 56], "incomplete R7 source population")
    return plan, reports_for(plan_ref, plan, lifetimes, observations), observations


def check_live_owner(plan):
    """Confirm the first-lifetime read still belongs to the running R7 owner."""
    import ctypes
    from ctypes import wintypes
    import runpy

    record = read(plan["installation_mutex"], R7_ROOT/"control-mutex-acquired.json")
    name = r"Local\AIRK-20261116-Focus1024-Reuse"
    require(record["name"] == name and 0 < record["owner_pid"] and record["at_ms"] <= plan["start_ms"] and
            not (R7_ROOT/"control-mutex-released.json").exists(), "R7 installation already retired or owner differs")
    mutex = HERE.parent/"cal_capture/grouped/installation_mutex.py"
    [source] = [s for s in plan["sources"] if s["path"] == str(mutex)]
    require(digest(mutex.read_bytes()) == source["sha256"], "original mutex observer source differs")
    api = ctypes.WinDLL("kernel32", use_last_error=True)
    api.OpenProcess.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
    api.OpenProcess.restype = wintypes.HANDLE
    api.WaitForSingleObject.argtypes = (wintypes.HANDLE, wintypes.DWORD)
    api.GetProcessTimes.argtypes = (wintypes.HANDLE, *([ctypes.POINTER(wintypes.FILETIME)] * 4))
    api.CloseHandle.argtypes = (wintypes.HANDLE,)
    handle = api.OpenProcess(0x100000 | 0x1000, False, record["owner_pid"])
    require(bool(handle), "original R7 installation owner process unavailable")
    try:
        times = [wintypes.FILETIME() for _ in range(4)]
        require(api.WaitForSingleObject(handle, 0) == 258 and api.GetProcessTimes(handle, *map(ctypes.byref, times)),
                "original R7 installation owner exited")
        born = ((times[0].dwHighDateTime << 32) | times[0].dwLowDateTime)//10000 - 11644473600000
        require(0 < born <= record["at_ms"], "original R7 installation owner PID was reused")
        try:
            with runpy.run_path(str(mutex))["InstallationMutex"](name):
                raise ValueError("original R7 installation mutex unexpectedly available")
        except BlockingIOError:
            require(api.WaitForSingleObject(handle, 0) == 258 and
                    not (R7_ROOT/"control-mutex-released.json").exists(), "R7 installation owner retired")
    finally:
        api.CloseHandle(handle)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan-sha256", required=True)
    parser.add_argument("--lifetime-sha256", action="append", required=True)
    parser.add_argument("--terminal", nargs=3, metavar=("OUTER_SHA", "FIXTURE_SHA", "RELEASE_SHA"))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    require(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode, "use checked Python -I -B -S")
    out = args.output
    require(out.is_absolute() and out.resolve(strict=True) == out and
            out.parent == R7_ROOT.parent and not any(out.iterdir()), "empty guarded campaign output required")
    plan_ref = reference(R7_ROOT/"lifetime-plan.json", args.plan_sha256)
    plan, reports, observations = collect(plan_ref, args.lifetime_sha256, args.terminal)
    if args.terminal is None:
        check_live_owner(plan)
    phases = []
    names = [f"phase-{report['phase']:02}-independent-inputs.json" for report in reports]
    original.check_outputs(out, [*names, "compact.json", "large.json", "projection-receipt.json"], created=False)
    for name, report in zip(names, reports, strict=True):
        path = out/name
        raw = json.dumps(report, allow_nan=False, separators=(",", ":")).encode()
        with path.open("xb") as file:
            file.write(raw)
        original.check_outputs(out, [name], created=True)
        phases.append({"path": str(path), "sha256": digest(raw)})
    models = {}
    for model, result in batches(plan_ref, plan, reports, observations, phases).items():
        path = out/f"{model}.json"
        raw = json.dumps(result, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode()
        require(len(raw) <= 128 << 20, "native compiler projection exceeds input limit")
        with path.open("xb") as file:
            file.write(raw)
        original.check_outputs(out, [path.name], created=True)
        models[model] = reference(path, digest(raw))
    receipt = {"schema": SCHEMA, "plan": plan_ref, "plan_sha256": plan_ref["sha256"],
               "launch_audit": plan["input_audit"], "service_sha256": SERVICE,
               "lifetimes": args.lifetime_sha256, "terminal": args.terminal,
               "acquired_sha256": plan["installation_mutex"]["sha256"], "models": models,
               "original_requests": len(observations), "NN_invocations": 0, "gain_certificates": 0,
               "source_sha256": digest(Path(__file__).read_bytes())}
    if args.terminal is None:
        check_live_owner(plan)
    with (out/"projection-receipt.json").open("x", encoding="utf-8") as file:
        json.dump(receipt, file, allow_nan=False)
    original.check_outputs(out, ["projection-receipt.json"], created=True)
    print(json.dumps(receipt, allow_nan=False))


if __name__ == "__main__":
    main()
