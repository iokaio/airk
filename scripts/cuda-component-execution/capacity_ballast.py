# SPDX-License-Identifier: Apache-2.0
"""Hold touched A6000 allocations in a bounded Windows Job while a reviewed native trial runs."""

import argparse
import ctypes as ct
from ctypes import wintypes as wt
import hashlib
import json
import os
from pathlib import Path
import runpy
import sys
import threading
import time
import uuid

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
sys.path.insert(0, str(HERE))
from capacity_ballast_worker import GIB, MIB, PINS, SMALL, SYSTEM, TIERS, UUID, WINDOW, bind, require, verify
from wddm_capacity_snapshot import snapshot
from wddm_process_samples import ProcessSampler

HELPER = ROOT / "scripts/cuda-native-execution/helpers/reference_owned.py"
OWNER_SHA = "2d7a31a64eaba745e32ed83adfbeedb75983ab6f9b5c4cc82714e9371c1895a1"
BALLAST_SECONDS = 300
SAMPLE_NS = 500_000_000
MIN_HELD_NS = 1_000_000_000


def pin(path):
    with path.open("rb") as stream:
        return {"path": str(path), "sha256": hashlib.file_digest(stream, "sha256").hexdigest()}


def natural(job):
    return (type(job.get("exit_code")) is int and job["exit_code"] == 0 and job.get("timed_out") is False
            and job.get("log_oversized") is False and job.get("owned_descendants_settled") is True
            and job.get("owned_job_final_active_processes") == 0 and job.get("owned_job_final_member_pids") == []
            and job.get("owned_job_forced_termination") is False and job.get("child_assigned_before_resume") is True
            and job.get("owned_total_processes") == 1 and "cleanup_refusal" not in job)


def resident(row, baseline, amount):
    dedicated = row["process"]["Dedicated Usage"]
    shared = row["process"]["Shared Usage"]
    committed = row["process"]["Total Committed"]
    return (all(type(value) is int and value >= 0 for value in (dedicated, shared, committed))
            and dedicated >= baseline["Dedicated Usage"] + amount and shared <= baseline["Shared Usage"]
            and committed >= dedicated)


def headroom(current, row, cuda, tier):
    card = current["nvidia_smi_mib"]
    require(card["uuid"] == "GPU-" + UUID and card["driver_model.current"] == "WDDM" and
            card["driver_version"] == "597.06" and tier in TIERS, "foreign GPU, driver or trial tier")
    free = {"nvidia_free": card["memory.free"] * MIB, "cuda_free_after_touch": cuda["free"],
            "wddm_free": current["dxgi"]["dedicated_video_bytes"] - row["adapter"]["Dedicated Usage"]}
    minimum = min(free.values())
    require(tier * GIB <= minimum <= tier * GIB + WINDOW and
            min(free["nvidia_free"], free["wddm_free"]) <= tier * GIB + WINDOW,
            "observed available memory is outside the preregistered tier window")
    return free


def wait_until(event, deadline, why):
    left = deadline - time.monotonic()
    require(left > 0 and event.wait(left), why)


class Gate:
    def __init__(self, tier, small, deadline, luid):
        self.tier, self.small, self.deadline, self.luid = tier, small, deadline, luid
        self.ready = threading.Event()
        self.end = threading.Event()
        self.lock = threading.Lock()
        self.latest = self.held = self.baseline = self.error = self.admission = None
        self.rows = []

    def authorize(self, candidate_pid, child_deadline):
        require(type(candidate_pid) is int and candidate_pid > 0 and time.monotonic() < child_deadline <= self.deadline
                and not self.small and self.admission is None, "one native admission per floor trial")
        captured = snapshot(None)
        with self.lock:
            require(time.monotonic() < child_deadline and self.error is None and not self.end.is_set()
                    and self.latest and time.monotonic_ns() - self.latest["end_monotonic_ns"] <= SAMPLE_NS
                    and captured["dxgi"]["luid"] == self.luid and
                    resident(self.latest, self.baseline, self.held["touched"]), "stale/nonresident pre-resume measurement")
            self.admission = {"candidate_pid":candidate_pid, "child_deadline":child_deadline, "observed_monotonic_ns": time.monotonic_ns(), "snapshot": captured,
                              "bytes": headroom(captured, self.latest, self.held, self.tier),
                              "sample": self.latest}
        return self.admission


def json_events(log):
    require(log.stat().st_size <= 65536, "unexpected worker output size")
    data = log.read_bytes()
    return [json.loads(line) for line in data.split(b"\n")[:-1] if line.strip()]


def terminal(messages, accepted, stopping, pid):
    progress = [v for v in messages if v["kind"] == "chunk"]
    stages = [v for v in messages if v["kind"] != "chunk"]
    errors = [v for v in stages if v["kind"] == "worker_failure"]
    if errors:
        require(len(errors) == 1 and stages[-1] == errors[0], "ambiguous CUDA worker error")
        # Return normally to the Job owner, including nonzero leader exit. This retains
        # actual natural/forced settlement separately from CUDA allocation/refusal.
        return {"worker_pid":pid, "messages":messages, "worker_error":errors[0]}
    require([v["kind"] for v in stages] == ["context","held","released"] and accepted and stopping and
            stages[-1]["freed_chunks"] == stages[-1]["allocated_chunks"] == len(progress) == stages[1]["chunks"]
            and stages[-1]["touched_bytes"] == stages[-1]["allocated_bytes"] == stages[1]["touched"],
            "missing natural allocation/release evidence")
    return {"worker_pid": pid, "messages": messages}


def monitor(api, info, deadline, gate, events, log, set_event):
    baseline_since = held_since = None
    opened = accepted = stopping = False
    last_counter = None
    try:
        with ProcessSampler(info, gate.luid) as sampler:
            while True:
                require(time.monotonic() < deadline, "original ballast Job expired")
                rc = api.WaitForSingleObject(info.process, 0)
                if rc == 0:
                    break
                require(rc == 258, "bad original ballast process handle")
                row = sampler.sample()
                gate.rows.append(row)
                require(len(gate.rows) <= 5000, "sample bound exceeded")
                messages = json_events(log)
                context = next((v for v in messages if v["kind"] == "context"), None)
                held = next((v for v in messages if v["kind"] == "held"), None)
                values = row["process"]
                now = row["end_monotonic_ns"]
                if context and not opened and all(type(v) is int and v >= 0 for v in values.values()):
                    if baseline_since is None:
                        baseline_since = now
                    if now - baseline_since >= MIN_HELD_NS:
                        gate.baseline = values.copy()
                        require(context["uuid"] == UUID and context["luid"] == gate.luid, "worker GPU differs")
                        require(set_event(events[0]), "could not authorize allocation")
                        opened = True
                if opened and not stopping and type(values["Shared Usage"]) is int:
                    require(values["Shared Usage"] <= gate.baseline["Shared Usage"],
                            "ballast shared usage increased during allocation or hold")
                if held and opened and not stopping:
                    ticks = row["counter_filetime_100ns"].get("process/Dedicated Usage")
                    if accepted:
                        require(type(ticks) is int and ticks > last_counter and gate.latest is not None and
                                now - gate.latest["end_monotonic_ns"] <= SAMPLE_NS,
                                "stale or interrupted ballast residency counters")
                    if type(ticks) is int:
                        last_counter = ticks
                    require(held["touched"] > 0, "zero ballast bytes")
                    if resident(row, gate.baseline, held["touched"]):
                        if held_since is None:
                            held_since = now
                        with gate.lock:
                            gate.held, gate.latest = held, row
                            if now - held_since >= MIN_HELD_NS:
                                accepted = True
                                gate.ready.set()
                    elif accepted:
                        raise ValueError("ballast dedicated residency lost or shared growth observed")
                    else:
                        held_since = None
                if gate.end.is_set() and not stopping:
                    require(set_event(events[1]), "could not request natural release")
                    stopping = True
                rc = api.WaitForSingleObject(info.process, 100)
                require(rc in (0, 258), "ballast process wait failed")
        messages = json_events(log)
        return terminal(messages, accepted, stopping, int(info.pid)), []
    except BaseException as error:
        with gate.lock:
            gate.error = str(error)
        gate.ready.set()
        set_event(events[1])
        set_event(events[0])
        raise


def run(out, tier, maximum, *, small=False, trial=None, calibration=False):
    require(os.name == "nt" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode and
            Path(sys.executable) in PINS, "checked isolated Windows Python required")
    require(not calibration or (small and maximum == 2*GIB), "bounded 2 GiB allocation calibration required")
    host_limit = (4 if calibration else 2) * GIB
    accounting = runpy.run_path(str(HERE/"ballast_host_commit.py")) if calibration else None
    before_host = accounting["host"]() if calibration else None
    if calibration:
        accounting["reserve"](before_host, host_limit)
    require(tier in TIERS and (maximum == (2*GIB if calibration else SMALL) if small else 0 < maximum <= (48 - tier)*GIB) and
            ((small and trial is None) or (not small and callable(trial))), "small proof or reviewed candidate required")
    require(out.is_absolute() and out.parent == CAMPAIGN and not out.exists() and
            out.name.startswith("compact-capacity-"), "fresh private campaign child required")
    deadline = time.monotonic() + BALLAST_SECONDS
    for path, digest in PINS.items():
        verify(path, digest)
    verify(HELPER, OWNER_SHA)
    owner = runpy.run_path(str(HELPER))
    component = runpy.run_path(str(ROOT / "scripts/cuda-component-execution.py"))
    base = component["base"]()
    parent, _ = base.helpers({"ownership_helper_root":str(base.MAINTAINED_HELPER_ROOT)})
    require(parent.worker.windows_sid() == base.SID and component["token_owner"](ct.c_void_p(-1)) == base.SID,
            "controller SID or default owner differs")
    guard = {"checked_guard":base.pin(ROOT/"scripts/dev/assert-human-execution.ps1"),
             "powershell":base.pin(Path("C:/Program Files/PowerShell/7/pwsh.exe"))}
    def audit(paths,created):
        component["guard_many"](guard, paths, created, deadline=deadline)
    initial = snapshot(None)
    require(initial["nvidia_smi_mib"]["driver_version"] == "597.06", "driver version differs")
    luid = initial["dxgi"]["luid"]
    gate = Gate(tier, small, deadline, luid)
    nonce = str(uuid.uuid4())
    kernel = ct.WinDLL(str(SYSTEM / "kernel32.dll"), use_last_error=True)
    create = bind(kernel, "CreateEventW", wt.HANDLE, ct.c_void_p, wt.BOOL, wt.BOOL, wt.LPCWSTR)
    signal = bind(kernel, "SetEvent", wt.BOOL, wt.HANDLE)
    close = bind(kernel, "CloseHandle", wt.BOOL, wt.HANDLE)
    audit([out],False)
    out.mkdir()
    audit([out],True)
    log, stdin = out / "worker.log", out / "stdin"
    audit([log, stdin, out/"execution.json"],False)
    worker = HERE / "capacity_ballast_worker.py"
    args = [sys.executable, "-I", "-B", "-S", worker, "--tier", tier, "--maximum", maximum,
            "--deadline-ns", int(deadline * 1e9), "--luid", luid, "--nonce", nonce]
    if small:
        args.append("--small")
    if calibration:
        args.append("--commit-calibration")
    args = list(map(str, args))
    image = pin(Path(sys.executable))
    result = {"complete": False, "tier_gib": tier, "small_model_free": small, "maximum_bytes": maximum,
              "original_deadline_monotonic": deadline, "initial": initial, "worker": pin(worker), "argv": args,
              "source_pins": [pin(p) for p in (Path(__file__), HERE/"wddm_process_samples.py",
                              HERE/"wddm_capacity_snapshot.py", HELPER)], "driver_and_runtime_pins": {str(p): v for p,v in PINS.items()},
              "host_equivalence": False, "qualification": False, "samples": gate.rows}
    if calibration:
        result.update(host_before=before_host, ballast_job_limit_bytes=host_limit,
                      commit_calibration=True, host_accounting_source=pin(HERE/"ballast_host_commit.py"))
    events = []
    thread = None
    try:
        for op in ("go", "stop"):
            ct.set_last_error(0)
            handle = create(None, True, False, "Local\\airk-ballast-" + nonce + "-" + op)
            require(handle and ct.get_last_error() != 183, "new event required")
            events.append(handle)
        def before(api, job, info, expires):
            active, count, total = owner["job_snapshot"](api, job)
            require(active == [int(info.pid)] and count == total == 1 and time.monotonic() < expires,
                    "wrong suspended worker Job")
            suspended = parent.suspended_authentication(api,info,deadline,expires,base.SID,image,owner["image_proof"])
            require(component["token_owner"](info.process) == base.SID, "ballast child default owner differs")
            return {"identity":suspended, "pid":int(info.pid)}
        def launch():
            try:
                with log.open("xb") as capture, stdin.open("xb+") as empty:
                    audit([log,stdin],True)
                    env = {k:v for k,v in os.environ.items() if not k.upper().startswith(("CUDA_", "PYTHON", "AIRK_", "ORT_"))}
                    env.update(PATH=str(SYSTEM), CUDA_CACHE_DISABLE="1")
                    result["job"] = owner["run_windows_owned_training"](
                        args, ROOT, capture, deadline-time.monotonic(), env, empty, host_limit,
                        before_resume=before, absolute_deadline=deadline,
                        after_resume=lambda a,j,i,d: monitor(a,i,d,gate,events,log,signal))
                if not natural(result["job"]):
                    refused = result["job"].get("before_GO", {}).get("worker_error", {})
                    raise ValueError(refused.get("message", "ballast Job did not exit naturally"))
            except BaseException as error:
                gate.error = str(error)
                result["worker_failure"] = str(error)
                gate.ready.set()
        thread = threading.Thread(target=launch)
        thread.start()
        wait_until(gate.ready, deadline, "ballast never established residency")
        require(gate.error is None and gate.held is not None, gate.error or "missing residency")
        result["held_snapshot"] = snapshot(None)
        if not small:
            result["trial"] = trial(gate.authorize)
            require(gate.admission is not None, "candidate did not request original pre-resume headroom gate")
    except BaseException as error:
        result["failure"] = str(error)
    finally:
        gate.end.set()
        if thread:
            thread.join(max(0, deadline-time.monotonic())+5)
        result["all_workers_joined"] = not thread or not thread.is_alive()
        if result["all_workers_joined"]:
            for handle in events:
                require(close(handle), "controller event close failed")
        result.update(admission=gate.admission, baseline=gate.baseline, held=gate.held, sample_error=gate.error)
        result["complete"] = (result["all_workers_joined"] and gate.error is None and "failure" not in result
                              and natural(result.get("job", {})) and time.monotonic() <= deadline)
        try:
            result["post_release"] = snapshot(None)
            if calibration:
                result["host_after"] = accounting["host"]()
        except Exception as error:
            result["complete"] = False
            result["post_release_failure"] = str(error)
        if time.monotonic() > deadline:
            result["complete"] = False
            result["whole_phase_expired"] = True
        outputs = list(out.rglob("*"))
        if outputs:
            component["guard_many"](guard, outputs, True)
        component["write_new"](guard,out/"execution.json",result)
    require(result["complete"], result.get("failure") or gate.error or "capacity observation incomplete")
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument("--small-model-free", action="store_true")
    modes.add_argument("--commit-calibration", action="store_true")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    run(args.output, 8, 2*GIB if args.commit_calibration else SMALL, small=True, calibration=args.commit_calibration)
