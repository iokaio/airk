# SPDX-License-Identifier: Apache-2.0
"""Place the owned R7 CPU reference on quiet physical cores using original CAL custody."""
from contextlib import ExitStack, contextmanager
import ctypes
from ctypes import wintypes as W
from datetime import datetime, timezone
import hashlib
import math
import os
from pathlib import Path
import re
import runpy
import sys
import threading
import time


HERE = Path(__file__).resolve().parent
SELECTED = HERE.parents[1]
ROOT = HERE.parents[5]
CAPTURE = SELECTED / "cal_capture"
CERTIFICATES = SELECTED / "cal_certificates"
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
R7 = CAMPAIGN / "cal100-focus1024-capture-r7"
PLAN_SHA = "c04be2262a32d8318f9aa222e73ea9bf078ad80d7975d907eebefe486895a2eb"
PREVIOUS = HERE.parents[3] / "analysis/development_margins/observe_cal_thread_affinity.py"
PREVIOUS_SHA = "7f37dcc3f9be734a964f561d9afc68b54ede19d036094b7fb7dab697f7d3f57f"
TIMING = CAPTURE / "timing/affinity.py"
MUTEX = CAPTURE / "grouped/installation_mutex.py"
OWNER = CAPTURE / "grouped/r6_capture.py"
WINDOWS_OWNER = ROOT / "scripts/arbiter-fp16-execution.py"
IMAGE_PROOF = ROOT / "scripts/cuda-native-execution/helpers/reference_owned.py"
SELECTOR = "service_runtime::evaluation_fixture_tests::actual_private_arbiter_fixture_software_v2"
PLAN_SCHEMA = "airk.private.cal100-r7-two-lifetime-plan/1"
FIXTURE_SCHEMA = "airk.private.cal100-r7-two-lifetime-fixture/1"
LIFETIMES = (tuple(range(7)), tuple(range(7, 13)))
TERMINAL_KEYS = ("outer", "fixture", "mutex")
FAILED_TERMINAL_KEYS = ("outer", "mutex", "first_lifetime", "phase", "request", "closed")
SAMPLE_SECONDS = 10
BELOW_NORMAL = 0x4000
STILL_RUNNING = 0x102
QUERY_AND_SYNCHRONIZE = 0x1000 | 0x100000
FILETIME_UNIX_EPOCH_MS = 11644473600000
P = ctypes.c_void_p
U64 = ctypes.c_uint64


def source_paths():
    """Dependencies another owner must include in the source seal."""
    return (Path(__file__).resolve(), PREVIOUS, TIMING, MUTEX, WINDOWS_OWNER, IMAGE_PROOF,
            SELECTED / "closure.py", CERTIFICATES / "reference_inputs.py",
            CERTIFICATES / "capture_closure.py", CERTIFICATES / "grouped_closure.py",
            CERTIFICATES / "r6_group.py")


def support():
    """Reuse the maintained physical-core policy and the Windows observation helpers."""
    sys.path.insert(0, str(CERTIFICATES))
    try:
        from capture_closure import settled
        from grouped_closure import check_mutex
        from r6_group import check_group
        from reference_inputs import checked, closures, parsed
    finally:
        sys.path.remove(str(CERTIFICATES))
    require(hashlib.sha256(PREVIOUS.read_bytes()).hexdigest() == PREVIOUS_SHA,
            "retained physical-core selection source changed")
    return Support(checked, closures["plain"], parsed, settled, check_mutex, check_group)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def file_identity(info):
    return info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns


class Support:
    def __init__(self, checked, plain, parsed, settled, check_mutex, check_group):
        self.checked, self.plain, self.parsed = checked, plain, parsed
        self.settled, self.check_mutex, self.check_group = settled, check_mutex, check_group
        self.previous = runpy.run_path(str(PREVIOUS))
        self.timing = runpy.run_path(str(TIMING))
        self.owner = runpy.run_path(str(WINDOWS_OWNER))
        self.image_proof = runpy.run_path(str(IMAGE_PROOF))["image_proof"]
        self.sid = self.owner["SID"]
        self.api, self.ok = self.timing["api"], self.timing["checked"]

    def identity(self, path=None, process=None):
        return self.owner["windows_owner"](path=path, process=process)

    def receipt(self, path, *, pin=None, sha=None):
        path = Path(path)
        _, before = self.plain(path)
        self.identity(path=path)
        require(before.st_size <= 1 << 20, "R7 custody receipt exceeds its byte bound")
        require(pin is None or Path(pin["path"]) == path, "different R7 receipt path")
        if pin is None:
            if sha is None:
                with path.open("rb") as stream:
                    raw = stream.read((1 << 20) + 1)
                require(len(raw) == before.st_size, "R7 receipt changed during discovery")
                sha = hashlib.sha256(raw).hexdigest()
            pin = {"path": str(path), "bytes": before.st_size, "sha256": sha}
        raw = self.checked(pin)
        _, after = self.plain(path)
        require(file_identity(before) == file_identity(after), "R7 receipt changed while observing custody")
        return self.parsed(raw), pin, before.st_mtime_ns // 1_000_000

    def mutex_type(self, plan):
        matches = [pin for pin in plan["sources"] if Path(pin["path"]) == MUTEX]
        require(len(matches) == 1, "R7 installation mutex source pin unavailable")
        self.checked(matches[0])
        scope = runpy.run_path(str(MUTEX))
        return scope["InstallationMutex"], scope["MUTEX_NAME"]


class Process:
    """Hold one process object with query and wait access for its whole observation."""

    def __init__(self, api, pid=None):
        self.api = api
        self.current = pid is None
        self.pid = os.getpid() if self.current else pid
        self.handle = (api("GetCurrentProcess", P)() if self.current else
                       api("OpenProcess", P, W.DWORD, W.BOOL, W.DWORD)(QUERY_AND_SYNCHRONIZE, False, pid))
        if not self.handle:
            raise ctypes.WinError(ctypes.get_last_error())

    def __enter__(self):
        return self

    def __exit__(self, *_):
        if not self.current:
            self.api("CloseHandle", W.BOOL, P)(self.handle)

    def wait(self, milliseconds=0):
        state = self.api("WaitForSingleObject", W.DWORD, P, W.DWORD)(self.handle, milliseconds)
        require(state in (0, STILL_RUNNING), "Windows process wait failed")
        return state

    def live(self):
        require(self.wait() == STILL_RUNNING, "original R7 process exited")
        pid = self.api("GetProcessId", W.DWORD, P)(self.handle)
        require(pid == self.pid, "process handle PID differs")

    def born(self):
        times = (W.FILETIME * 4)()
        read = self.api("GetProcessTimes", W.BOOL, P, P, P, P, P)
        require(read(self.handle, *[ctypes.byref(times, offset * ctypes.sizeof(W.FILETIME)) for offset in range(4)]),
                "Windows process creation time unavailable")
        first = times[0]
        return ((first.dwHighDateTime << 32) | first.dwLowDateTime) // 10000 - FILETIME_UNIX_EPOCH_MS

    def parent(self):
        # PROCESS_BASIC_INFORMATION has six pointer-sized fields on the supported 64-bit host.
        info = (ctypes.c_size_t * 6)()
        query = ctypes.WinDLL("ntdll").NtQueryInformationProcess
        query.argtypes, query.restype = [P, W.ULONG, P, W.ULONG, P], W.LONG
        require(query(self.handle, 0, ctypes.byref(info), ctypes.sizeof(info), None) == 0
                and info[4] == self.pid and info[5] > 0, "Windows process ancestry unavailable")
        return int(info[5])

    def in_job(self):
        result = W.BOOL()
        query = self.api("IsProcessInJob", W.BOOL, P, P, ctypes.POINTER(W.BOOL))
        require(query(self.handle, None, ctypes.byref(result)) and result.value, "process is outside its owned Job")

    def masks(self):
        mask, system = U64(), U64()
        fn = self.api("GetProcessAffinityMask", W.BOOL, P, ctypes.POINTER(U64), ctypes.POINTER(U64))
        require(fn(self.handle, ctypes.byref(mask), ctypes.byref(system)) and mask.value and system.value,
                "Windows process affinity unavailable")
        return mask.value, system.value

    def priority(self):
        result = self.api("GetPriorityClass", W.DWORD, P)(self.handle)
        require(result != 0, "Windows process priority unavailable")
        return result

    def image(self, proof, pin, plain):
        require(isinstance(pin, dict) and set(pin) == {"path", "bytes", "sha256"}
                and type(pin["bytes"]) is int and pin["bytes"] > 0
                and re.fullmatch(r"[0-9a-f]{64}", pin["sha256"]), "exact process image pin required")
        target = Path(pin["path"])
        _, before = plain(target)
        require(before.st_size == pin["bytes"], "process image length differs")
        library = ctypes.WinDLL("kernel32", use_last_error=True)
        library.QueryFullProcessImageNameW = self.api(
            "QueryFullProcessImageNameW", W.BOOL, P, W.DWORD, W.LPWSTR, ctypes.POINTER(W.DWORD))
        image = proof(library, self.handle, pin)
        require(file_identity(plain(target)[1]) == file_identity(before), "process image changed during authentication")
        return image

    def place(self, mask):
        require(self.current, "placement may only change the current reference worker")
        priority = self.api("SetPriorityClass", W.BOOL, P, W.DWORD)
        affinity = self.api("SetProcessAffinityMask", W.BOOL, P, U64)
        require(affinity(self.handle, mask) and priority(self.handle, BELOW_NORMAL),
                "Windows rejected the reference worker placement")
        require(self.masks()[0] == mask and self.priority() == BELOW_NORMAL,
                "Windows did not retain BelowNormal reference worker placement")


def lifetime_receipts(support, plan, anchor, group, *, request_pin=None):
    directory = R7 / f"lifetime-{group:02}"
    fixture = directory / "fixture"
    request, pin, _ = support.receipt(directory / "live-request.json", pin=request_pin)
    start, _, _ = support.receipt(directory / "fixture-start.json", pin=request["start_receipt"])
    ready, _, ready_ms = support.receipt(fixture / "ready.json", pin=request["ready"])
    bindings, _, _ = support.receipt(fixture / "enable-context.bindings.json", pin=request["bindings"])
    support.receipt(fixture / "context-request.json", pin=request["answer"])
    health = ready["health"]
    image = plan["candidate_service"]["image"]
    require(request["schema"] == "airk.private.cal100-live-fixture/1"
            and request["fixture_root"] == start["fixture_root"] == str(fixture)
            and start["schema"] == "airk.private.cal100-fixture-start/1"
            and start["command"] == [image["path"], "--ignored", "--exact", SELECTOR, "--test-threads", "1"]
            and start["input_audit_sha256"] == plan["input_audit"]["sha256"]
            and type(start["start_ms"]) is int and plan["start_ms"] <= start["start_ms"] <= ready_ms
            and ready_ms < plan["deadline_ms"] and start["deadline_ms"] == start["start_ms"] + 10800000
            and type(start["pid"]) is int and 0 < start["pid"] <= 0xffffffff
            and ready["schema"] == "airk.private-arbiter-fixture/1"
            and ready["evaluation"] is True and ready["release_qualified"] is False
            and ready["control_root"] == anchor["control_root"]
            and ready["inputs_sha256"] == anchor["inputs_sha256"] and ready["profiles"] == anchor["profiles"]
            and health["process_id"] == start["pid"] and health["executable_sha256"] == image["sha256"]
            and health["user"] == support.sid and health["policy"]["accepting"] is True
            and bindings["instance"] == health["instance"] == health["policy"]["service"]
            and re.fullmatch(r"[0-9a-f]{32}", health["instance"])
            and health["instance"] != anchor["health"]["instance"],
            "R7 fixture start, readiness, instance or installation custody differs")
    return start, ready, ready_ms, pin


def original_owner_argv(outer):
    argv = outer["argv"]
    return (argv[1:5] == ["-I", "-B", "-S", str(OWNER)]
            and argv[5:7] == ["--output", str(R7)] and argv.count("--resume-r6") == 1
            and argv.count("--owned-plan-sha256") == 1
            and argv[argv.index("--owned-plan-sha256") + 1:][:1] == [PLAN_SHA])


def naturally_failed(owner):
    return (type(owner.get("exit_code")) is int and owner["exit_code"] == 1
            and owner.get("timed_out") is False and owner.get("log_oversized") is False
            and owner.get("child_assigned_before_resume") is True and owner.get("owned_descendants_settled") is True
            and owner.get("owned_job_forced_termination") is False and owner.get("owned_job_final_active_processes") == 0
            and owner.get("owned_job_final_member_pids") == [] and "cleanup_refusal" not in owner)


def native_closed(native, ready):
    return (native["schema"] == "airk.private-arbiter-fixture-closed/1"
            and native["shutdown_completed"] is True and native["service_lease_reacquired"] is True
            and native["control_root"] == ready["control_root"]
            and native["instance"] == ready["health"]["instance"])


def failed_terminal_receipts(support, plan, plan_pin, anchor, terminal):
    outer, _, outer_ms = support.receipt(R7 / "outer-owned.json", sha=terminal["outer"])
    require(naturally_failed(outer) and original_owner_argv(outer),
            "naturally failed original R7 outer Job required")
    first = R7 / "lifetime-00"
    lifetime, _, _ = support.receipt(first / "lifetime-closure.json", sha=terminal["first_lifetime"])
    require(lifetime["plan"] == plan_pin and lifetime["prior"] == plan["installation"]["closed"]
            and lifetime["end_ms"] < plan["deadline_ms"], "original first R7 lifetime differs")
    support.check_group(first, lifetime, 0, plan, plan["start_ms"])
    start0, ready0, ready0_ms, _ = lifetime_receipts(support, plan, anchor, 0, request_pin=lifetime["live_request"])
    native0, _, _ = support.receipt(first / "fixture/closed.json", pin=lifetime["closed"])
    require(native_closed(native0, ready0) and start0["start_ms"] == lifetime["start_ms"]
            and ready0_ms <= lifetime["end_ms"] and native0["instance"] == lifetime["instance"],
            "first R7 native fixture did not close before its next lifetime")

    last = R7 / "lifetime-01"
    failed, _, _ = support.receipt(last / "phase-12.owned.json", sha=terminal["phase"])
    request, request_pin, _ = support.receipt(last / "live-request.json", sha=terminal["request"])
    start1, ready1, ready1_ms, _ = lifetime_receipts(support, plan, anchor, 1, request_pin=request_pin)
    native1, _, native1_ms = support.receipt(last / "fixture/closed.json", sha=terminal["closed"])
    require(naturally_failed(failed) and failed["phase"] == 12 and failed["request_sha256"] == request_pin["sha256"]
            and request["fixture_root"] == start1["fixture_root"] and failed["deadline_ms"] == plan["deadline_ms"]
            and lifetime["end_ms"] <= start1["start_ms"] <= ready1_ms <= failed["start_ms"] < failed["end_ms"]
            <= native1_ms <= outer_ms <= plan["deadline_ms"] and native_closed(native1, ready1)
            and native1["instance"] != native0["instance"],
            "failed R7 phase or its native cleanup differs")
    support.receipt(R7 / "control-mutex-released.json", sha=terminal["mutex"])
    support.check_mutex(R7, plan, terminal["outer"], terminal["mutex"], outer_ms)
    return [(start0, ready0_ms), (start1, ready1_ms)]


def terminal_receipts(support, plan, plan_pin, anchor, terminal):
    outer, _, _ = support.receipt(R7 / "outer-owned.json", sha=terminal["outer"])
    fixture, _, _ = support.receipt(R7 / "fixture-closure.json", sha=terminal["fixture"])
    support.receipt(R7 / "control-mutex-released.json", sha=terminal["mutex"])
    require(support.settled(outer) and original_owner_argv(outer)
            and fixture["schema"] == FIXTURE_SCHEMA and fixture["plan"] == plan_pin
            and fixture["ordered_questions"] == 100 and len(fixture["lifetimes"]) == len(LIFETIMES),
            "successful original R7 outer Job and full fixture closure required")
    previous, prior, instances, starts = plan["start_ms"], plan["installation"]["closed"], set(), []
    for group, lifetime_pin in enumerate(fixture["lifetimes"]):
        directory = R7 / f"lifetime-{group:02}"
        lifetime, _, _ = support.receipt(directory / "lifetime-closure.json", pin=lifetime_pin)
        require(lifetime["plan"] == plan_pin and lifetime["prior"] == prior
                and lifetime["publication"] == fixture["publication"] and lifetime["end_ms"] <= plan["deadline_ms"],
                "R7 terminal lifetime custody differs")
        support.check_group(directory, lifetime, group, plan, previous)
        start, ready, ready_ms, _ = lifetime_receipts(support, plan, anchor, group, request_pin=lifetime["live_request"])
        native, _, _ = support.receipt(directory / "fixture/closed.json", pin=lifetime["closed"])
        require(start["start_ms"] == lifetime["start_ms"] and ready_ms <= lifetime["end_ms"]
                and native_closed(native, ready) and native["instance"] == lifetime["instance"]
                and native["instance"] not in instances, "R7 native service or exclusive lease did not settle")
        instances.add(native["instance"])
        starts.append((start, ready_ms))
        previous, prior = lifetime["end_ms"], lifetime_pin
    support.check_mutex(R7, plan, terminal["outer"], terminal["mutex"], previous)
    return starts


def require_retired(support, starts):
    for start, ready_ms in starts:
        try:
            service = Process(support.api, start["pid"])
        except OSError as error:
            if getattr(error, "winerror", None) != 87:
                raise
        else:
            with service:
                require(service.wait() == 0 or service.born() > ready_ms,
                        "original R7 CAL PID still has a live native process")


def unclosed(group):
    directory = R7 / f"lifetime-{group:02}"
    return not any(path.exists() for path in (R7 / "control-mutex-released.json", R7 / "fixture-closure.json",
                                              directory / "lifetime-closure.json", directory / "fixture/closed.json"))


def ensure_busy(mutex):
    try:
        with mutex():
            pass
    except BlockingIOError:
        return
    raise ValueError("original R7 installation mutex is no longer held")


@contextmanager
def cal_custody(support, plan, plan_pin, cal_pid, terminal, deadline):
    anchor_pin = plan["installation"]["ready"]
    anchor, _, _ = support.receipt(Path(anchor_pin["path"]), pin=anchor_pin)
    acquisition, _, _ = support.receipt(R7 / "control-mutex-acquired.json", pin=plan["installation_mutex"])
    mutex, name = support.mutex_type(plan)
    require(acquisition["name"] == name and type(acquisition["owner_pid"]) is int and acquisition["owner_pid"] > 0
            and 0 < acquisition["at_ms"] <= plan["start_ms"], "original R7 installation acquisition differs")
    if terminal is not None:
        failed = "first_lifetime" in terminal
        receipt_reader = failed_terminal_receipts if failed else terminal_receipts
        mode = "failed_terminal" if failed else "settled"
        with mutex():
            starts = receipt_reader(support, plan, plan_pin, anchor, terminal)
            def check(reserve=0):
                require(time.monotonic() + reserve < deadline, "original reference worker deadline expired")
                require_retired(support, starts)
            check()
            yield None, {"mode": mode, "plan": plan_pin, "terminal": dict(terminal)}, check
        return

    candidates = [group for group in range(len(LIFETIMES)) if unclosed(group)
                  and (R7 / f"lifetime-{group:02}/live-request.json").is_file()]
    require(len(candidates) == 1, "exactly one original R7 ready lifetime must be open")
    group, = candidates
    start, ready, ready_ms, request_pin = lifetime_receipts(support, plan, anchor, group)
    require(start["pid"] == cal_pid, "requested CAL PID differs from the original R7 ready lifetime")
    with ExitStack() as handles:
        service = handles.enter_context(Process(support.api, cal_pid))
        owner = handles.enter_context(Process(support.api, acquisition["owner_pid"]))
        native_parent = handles.enter_context(Process(support.api, service.parent()))
        born = service.born()
        parent_born = native_parent.born()
        require(native_parent.parent() == owner.pid and 0 < owner.born() <= acquisition["at_ms"]
                and plan["start_ms"] <= parent_born <= start["start_ms"] <= born <= ready_ms,
                "CAL PID creation or native Job ancestry differs from original R7 lifetime")
        for process in (service, owner, native_parent):
            process.live()
            support.identity(process=process.handle)
        service.in_job()
        native_parent.in_job()
        image = service.image(support.image_proof, plan["candidate_service"]["image"], support.plain)

        def check(reserve=0):
            require(time.monotonic() + reserve < deadline
                    and time.time_ns() // 1_000_000 + reserve * 1000 < plan["deadline_ms"],
                    "original reference worker or R7 capture deadline expired")
            for process in (service, owner, native_parent):
                process.live()
            require(unclosed(group), "original R7 ready lifetime closed during observation")
            ensure_busy(mutex)

        check()
        yield service, {"mode": "live", "plan": plan_pin, "group": group, "pid": cal_pid,
                        "image": image["path"], "image_sha256": image["sha256"], "born_ms": born,
                        "owner_sid": support.sid, "native_parent_pid": native_parent.pid, "owner_pid": owner.pid,
                        "instance": ready["health"]["instance"], "live_request": request_pin}, check


def thread_snapshot(support, process):
    api = support.api
    open_thread = api("OpenThread", P, W.DWORD, W.BOOL, W.DWORD)
    thread_owner = api("GetProcessIdOfThread", W.DWORD, P)
    thread_times = api("GetThreadTimes", W.BOOL, P, P, P, P, P)
    group_affinity = api("GetThreadGroupAffinity", W.BOOL, P, P)
    ideal_processor = api("GetThreadIdealProcessorEx", W.BOOL, P, P)
    next_thread = api("Thread32Next", W.BOOL, P, P)
    close = api("CloseHandle", W.BOOL, P)
    snapshot = api("CreateToolhelp32Snapshot", P, W.DWORD, W.DWORD)(4, 0)
    require(snapshot and snapshot != P(-1).value, "Windows thread snapshot unavailable")
    try:
        entry = support.previous["Thread"]()
        entry.size = ctypes.sizeof(entry)
        present = api("Thread32First", W.BOOL, P, P)(snapshot, ctypes.byref(entry))
        require(present, "Windows thread enumeration unavailable")
        rows = {}
        while present:
            if entry.pid == process.pid:
                handle = open_thread(0x40, False, entry.tid)
                if handle:
                    try:
                        created, exited, kernel, user = (U64() for _ in range(4))
                        group, ideal = support.previous["Group"](), support.previous["Processor"]()
                        if (thread_owner(handle) == process.pid
                                and thread_times(handle, *[ctypes.byref(x) for x in (created, exited, kernel, user)])
                                and group_affinity(handle, ctypes.byref(group))
                                and ideal_processor(handle, ctypes.byref(ideal))):
                            require(group.group == ideal.group == 0, "unexpected CAL thread processor group")
                            rows[entry.tid] = (created.value, kernel.value + user.value, group.mask,
                                               ideal.number, entry.base_priority)
                    finally:
                        close(handle)
            present = next_thread(snapshot, ctypes.byref(entry))
        require(ctypes.get_last_error() == 18 and rows, "incomplete Windows process thread census")
        return rows
    finally:
        close(snapshot)


def activity(initial, final):
    busy = []
    for (first_idle, first_total), (last_idle, last_total) in zip(initial, final, strict=True):
        idle, total = last_idle - first_idle, last_total - first_total
        require(total > 0 and 0 <= idle <= total, "invalid Windows CPU activity counters")
        busy.append(round(100 * (1 - idle / total), 1))
    return busy


def sample(support, worker, cal, evidence, check):
    observed = cal if cal is not None else worker
    mask, system = observed.masks()
    worker_mask, worker_system = worker.masks()
    physical = support.timing["relation"](0)
    require(len(physical) == 32 and all(bits.bit_count() == 2 for bits in physical)
            and sum(physical) == system == worker_system == (1 << 64) - 1
            and all(a & b == 0 for index, a in enumerate(physical) for b in physical[index + 1:]),
            "original 64-logical-CPU, 32-physical-core topology required")
    start = datetime.now(timezone.utc).isoformat()
    counters = support.timing["load_counters"]
    a, cpu_a = thread_snapshot(support, observed), counters()
    check(SAMPLE_SECONDS)
    if cal is None:
        threading.Event().wait(SAMPLE_SECONDS)
    else:
        require(cal.wait(SAMPLE_SECONDS * 1000) == STILL_RUNNING, "R7 CAL exited during CPU activity sample")
    b, cpu_b = thread_snapshot(support, observed), counters()
    check()
    require(observed.masks() == (mask, system) and worker.masks() == (worker_mask, worker_system),
            "process affinity changed during original CPU sample")
    hot = []
    for tid in a.keys() & b.keys():
        previous, current = a[tid], b[tid]
        if previous[0] == current[0]:
            require(current[1] >= previous[1], "Windows thread CPU time regressed")
            hot.append({"tid": tid, "cpu_seconds": (current[1] - previous[1]) / 1e7, "group": 0,
                        "mask": hex(current[2]), "ideal": [0, current[3]], "base_priority": current[4]})
    require(hot, "no stable threads in the original CPU observation")
    return dict(evidence, pid=observed.pid, start_utc=start, seconds=SAMPLE_SECONDS,
                process_mask=hex(mask), system_mask=hex(system),
                affected_worker_allowed_mask=hex(worker_mask), priority_class=observed.priority(),
                physical_siblings=[[i for i in range(64) if bits & (1 << i)] for bits in physical],
                machine_cpu_busy_percent=activity(cpu_a, cpu_b), initial_threads=len(a), final_threads=len(b),
                threads=sorted(hot, key=lambda row: (-row["cpu_seconds"], row["tid"])))


def validate_args(cal_pid, count, deadline, terminal):
    require(count == 2 and type(count) is int and type(deadline) in (float, int) and math.isfinite(deadline),
            "original two-core worker placement and finite deadline required")
    if cal_pid is None:
        require(isinstance(terminal, dict) and set(terminal) in (set(TERMINAL_KEYS), set(FAILED_TERMINAL_KEYS))
                and all(isinstance(sha, str) and re.fullmatch(r"[0-9a-f]{64}", sha) for sha in terminal.values()),
                "exact original R7 successful or failed terminal custody hashes required")
    else:
        require(type(cal_pid) is int and 0 < cal_pid <= 0xffffffff and terminal is None,
                "one exact original R7 live CAL PID is required")


def place_optional_worker(cal_pid, count, *, deadline, worker_image, terminal=None):
    """Authenticate the CAL operating state, then place only this same-human owned worker."""
    validate_args(cal_pid, count, deadline, terminal)
    require(os.name == "nt" and ctypes.sizeof(P) == 8 and time.monotonic() + SAMPLE_SECONDS < deadline,
            "64-bit Windows worker with the original observation time required")
    helpers = support()
    plan, plan_pin, _ = helpers.receipt(R7 / "lifetime-plan.json", sha=PLAN_SHA)
    require(plan["schema"] == PLAN_SCHEMA and plan["phase_groups"] == [list(group) for group in LIFETIMES],
            "original R7 two-lifetime plan required")
    with Process(helpers.api) as worker:
        worker.live()
        worker.in_job()
        helpers.identity(process=worker.handle)
        require(Path(worker_image["path"]).resolve(strict=True) == Path(sys.executable).resolve(strict=True),
                "current reference interpreter differs from the pinned Python image")
        worker_proof = worker.image(helpers.image_proof, worker_image, helpers.plain)
        with cal_custody(helpers, plan, plan_pin, cal_pid, terminal, deadline) as (cal, custody, check):
            observation = sample(helpers, worker, cal, custody, check)
            before = int(observation["affected_worker_allowed_mask"], 16)
            cpus = helpers.previous["pick_cpus"](dict(observation, system_mask=hex(before)), count)
            selected = sum(1 << cpu for cpu in cpus)
            require(worker.masks()[0] == before and before & selected == selected,
                    "quiet physical cores differ from the reference worker's allowed affinity")
            check()
            worker.place(selected)
            check()
    return {"cal_observation": observation, "logical_cpus": cpus, "initial_worker_mask": hex(before),
            "worker_mask": hex(selected), "priority_class": BELOW_NORMAL,
            "worker": {"pid": worker.pid, "image": worker_proof, "owner_sid": helpers.sid}}
