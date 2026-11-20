# SPDX-License-Identifier: Apache-2.0
"""Own 13 ordered CAL phase children against one already-bound native fixture."""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import time

from prepare import CAMPAIGN, ROOT, pin, read

HERE = Path(__file__).resolve().parent
NODE_SHA = "ba4e6d110e8c1592a1ecd390f6b05f3da124b13871a5be62b341a07a853c6c32"
AUDIT_SHA = "39e36a6fcc40f28412ff97ede00917b3bcd5147675c70510691b01e901607287"


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def link(path):
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": digest}


def owned_module():
    path = ROOT / "scripts/provision-static-onnx-runtime.py"
    spec = importlib.util.spec_from_file_location("cal_windows_execution", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def summary(log, phase, expected_ids, instance, publication):
    require(log.stat().st_size <= 64 << 20, "phase transcript exceeds owner limit")
    rows, closes, probes, pipeline_searches = [], [], 0, 0
    with log.open("rb") as stream:
        for line in stream:
            event = json.loads(line)
            if event["kind"] == "cal_question":
                evidence = event["evidence"]
                rows.append(evidence["id"])
                require(len(evidence["probes"]) <= 5, "more than five counterfactuals")
                probes += len(evidence["probes"])
                require(type(evidence["pipeline_searches"]) is int and evidence["pipeline_searches"] >= 0,
                        "invalid observed Search count")
                pipeline_searches += evidence["pipeline_searches"]
            elif event["kind"] == "phase_complete":
                closes.append(event)
    require(rows == expected_ids and len(closes) == 1 and closes[0] == {
        "kind": "phase_complete", "phase": phase, "ids": expected_ids,
        "instance": instance, "publication": publication}, "incomplete original census")
    return probes, pipeline_searches


def capture(request_path, request_sha, output, node):
    require(os.name == "nt" and sys.dont_write_bytecode and sys.flags.no_site, "checked Windows Python -B -S required")
    require(request_path.is_absolute() and request_path.is_relative_to(CAMPAIGN) and
            output.is_absolute() and output.is_relative_to(CAMPAIGN) and
            output.resolve(strict=True) == output and not any(output.iterdir()), "fresh checked output directory required")
    request = read(request_path, request_sha)
    require(request["schema"] == "airk.private.cal100-live-fixture/1", "unknown live fixture request")
    audit = read(CAMPAIGN / "cal100-launch-input-audit-r1.json", AUDIT_SHA)
    ids = read(Path(audit["census"]["path"]), audit["census"]["sha256"])["question_ids"]
    root = Path(request["fixture_root"])
    require(root.is_absolute() and root.is_relative_to(CAMPAIGN) and root.resolve(strict=True) == root,
            "fixture root outside campaign")
    clock = request["start_receipt"]
    bound = read(Path(clock["path"]), clock["sha256"])
    require(bound["schema"] == "airk.private.cal100-fixture-start/1" and bound["fixture_root"] == str(root) and
            bound["input_audit_sha256"] == AUDIT_SHA and bound["deadline_ms"] - bound["start_ms"] == 10800000 and
            bound["start_ms"] <= time.time_ns() // 1000000 < bound["deadline_ms"], "invalid original fixture lifetime")
    deadline = time.monotonic() + (bound["deadline_ms"] / 1000 - time.time())
    ready = read(Path(request["ready"]["path"]), request["ready"]["sha256"])
    bindings = read(Path(request["bindings"]["path"]), request["bindings"]["sha256"])
    require(ready["health"]["process_id"] == bound["pid"] and ready["health"]["instance"] == bindings["instance"] and
            ready["health"]["executable_sha256"] == audit["inputs"]["service"]["sha256"] and
            len(bindings["bindings"]) == 1 and not (root / "closed.json").exists(),
            "live service does not match original fixture")
    instance, publication = bindings["instance"], bindings["bindings"][0]["publication"]
    pin(node, NODE_SHA)
    owner = owned_module()
    source = [link(HERE / n) for n in ("worker.cjs", "phase.cjs", "controller.py", "prepare.py")]
    source.append(link(Path(owner.__file__)))
    closures = []
    total_probes = total_searches = 0
    for phase in range(13):
        for ref in source:
            pin(Path(ref["path"]), ref["sha256"])
        pin(request_path, request_sha)
        remaining = min(1800, deadline - time.monotonic(), bound["deadline_ms"]/1000 - time.time())
        require(remaining > 125, "original fixture lifetime cannot admit another call")
        log = output / f"phase-{phase:02}.jsonl"
        command = [node, HERE / "worker.cjs", request_path, request_sha, phase, remaining]
        values = {k: v for k, v in os.environ.items() if k.upper() not in {"NODE_OPTIONS", "NODE_PATH"}}
        values.update(AIRK_CAL100_LIVE_REQUEST=str(request_path), AIRK_CAL100_PYTHON=sys.executable)
        result = owner.run_evaluation_fixture(command, ROOT, log, remaining, values)
        result.update({"phase": phase, "transcript": link(log), "request_sha256": request_sha, "sources": source})
        closure = output / f"phase-{phase:02}.owned.json"
        with closure.open("x", encoding="utf-8", newline="\n") as stream:
            json.dump(result, stream, sort_keys=True)
        require(owner.successful(result) and result["owned_job_final_active_processes"] == 0 and
                result["owned_job_final_member_pids"] == [], "phase failed or did not settle; retained original result")
        probes, searches = summary(log, phase, ids[8*phase:8*phase+8], instance, publication)
        total_probes += probes
        total_searches += searches
        closures.append(link(closure))
    for ref in source:
        pin(Path(ref["path"]), ref["sha256"])
    require(total_searches > 0, "no actual CAL Search proved the served MiniLM publication")
    require(total_probes <= 500 and time.monotonic() < deadline, "original census budget exceeded")
    return {"schema":"airk.private.cal100-owned-phase-census/1", "instance": instance, "publication": publication,
            "phase_closures": closures, "probes": total_probes, "ordered_questions": 100, "pipeline_searches": total_searches,
            "request_sha256": request_sha, "original_start": clock, "fixture_shutdown_verified": False,
            "independent_certificates": False, "fitted": False}


def verify_worker_job(pid):
    import ctypes
    from ctypes import wintypes

    require(os.name == "nt" and pid == os.getppid(), "actual parent required")
    api = ctypes.WinDLL("kernel32", use_last_error=True)
    api.OpenProcess.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
    api.OpenProcess.restype = wintypes.HANDLE
    api.IsProcessInJob.argtypes = (wintypes.HANDLE, wintypes.HANDLE, ctypes.POINTER(wintypes.BOOL))
    api.CloseHandle.argtypes = (wintypes.HANDLE,)
    handle = api.OpenProcess(0x1000, False, pid)
    require(handle, "cannot query phase parent")
    try:
        member = wintypes.BOOL()
        require(api.IsProcessInJob(handle, None, ctypes.byref(member)) and member.value,
                "phase worker is outside its required Job")
    finally:
        api.CloseHandle(handle)


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--verify-worker-job":
        verify_worker_job(int(sys.argv[2]))
        sys.exit(0)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--live-request", type=Path, required=True)
    parser.add_argument("--request-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--node", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(capture(args.live_request, args.request_sha256, args.output, args.node), sort_keys=True))
