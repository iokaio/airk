# SPDX-License-Identifier: Apache-2.0
"""Observe Resource20's native journal while the real Windows Job is held."""

import ctypes
from ctypes import wintypes as w
import json
from pathlib import Path
import time

JOURNAL_BYTES = 128 << 10
OPERATION_NS = 120_000_000_000
FIELDS = {
    "schema", "sequence", "identity", "kind", "ordinal", "operation_input_digest",
    "phase", "clock", "started_monotonic_ns", "expires_monotonic_ns",
    "observed_monotonic_ns", "native_call_elapsed_us", "native_success",
}
ORDER = (("load", 0), ("warmup", 0), *(("sample", index) for index in range(1, 21)))


def require(condition, message):
    if not condition:
        raise ValueError(message)


class Operations:
    def __init__(self, identity, digest, whole_expires_ns):
        self.identity = identity
        self.digest = digest
        self.whole_expires_ns = whole_expires_ns
        self.raw = b""
        self.events = []
        self.completed = 0

    def expiry(self):
        if self.events:
            return self.events[-1]["expires_monotonic_ns"]
        return self.whole_expires_ns

    def check_expiry(self, now):
        require(now < self.expiry(), "original potential native operation or whole expiry reached")

    def feed(self, raw, now):
        require(len(raw) <= JOURNAL_BYTES and raw.startswith(self.raw),
                "operation journal changed prefix or exceeded its bound")
        end = raw.rfind(b"\n") + 1
        for line in raw[:end].splitlines()[len(self.events):]:
            event = json.loads(line)
            self.observe(event, now)
        self.raw = raw

    def observe(self, event, now):
        require(set(event) == FIELDS and event["schema"] == "airk.arbiter.native-operation/1"
                and event["identity"] == self.identity and event["clock"] == "windows_performance_counter",
                "operation journal schema, current PID/image/input or clock differs")
        unsigned = [event[key] for key in ("sequence", "ordinal", "started_monotonic_ns",
                                           "observed_monotonic_ns", "expires_monotonic_ns")]
        require(all(type(value) is int and value >= 0 for value in unsigned), "unsigned operation fields required")
        sequence, ordinal, start, observed, expiry = unsigned
        prior = self.events[-1] if self.events else None
        require(sequence == len(self.events) + 1 and 0 < start <= observed <= now
                and expiry == min(start + OPERATION_NS, self.whole_expires_ns)
                and observed < expiry, "operation order, original allowance or observed QPC differs")
        require(self.completed < len(ORDER) and (event["kind"], ordinal) == ORDER[self.completed]
                and event["operation_input_digest"] == (None if self.completed == 0 else self.digest),
                "unexpected native operation kind, ordinal or compiled input")
        if event["phase"] == "started":
            require(prior is None or (prior["phase"] == "returned" and prior["native_success"] is True
                                      and start >= prior["observed_monotonic_ns"]),
                    "operation overlapped or followed an error")
            require(start == observed and event["native_success"] is None
                    and event["native_call_elapsed_us"] is None, "prospective start cannot claim a result")
            self.check_expiry(now)
        else:
            require(prior is not None and prior["phase"] == "started"
                    and event["kind"] == prior["kind"] and ordinal == prior["ordinal"]
                    and start == prior["started_monotonic_ns"]
                    and expiry == prior["expires_monotonic_ns"], "return changed the matching original start")
            elapsed = event["native_call_elapsed_us"]
            if event["phase"] == "returned":
                require(type(event["native_success"]) is bool and type(elapsed) is int
                        and 0 <= elapsed <= (observed - start) // 1000, "invalid native return")
                self.completed += 1
            else:
                require(event["phase"] == "not_dispatched" and event["native_success"] is None
                        and elapsed is None, "unknown operation phase")
            self.check_expiry(now)
        self.events.append(event)
        self.check_expiry(now)

    def prove_exit(self, code):
        if code == 0:
            require(self.completed == len(ORDER) and len(self.events) == 2 * len(ORDER)
                    and all(event["phase"] != "returned" or event["native_success"] is True
                            for event in self.events) and self.raw.endswith(b"\n"),
                    "zero-exit child omitted the exact 44-event population")

    def proof(self):
        return {"observed_events": list(self.events), "completed_return_markers": self.completed,
                "frozen_operations": len(ORDER), "journal_bytes": len(self.raw),
                "returned_marker_alone_clears_expiry": False,
                "whole_deadline_not_renewed": True}


class WindowsWatch:
    def __init__(self, journal, log, result, state, audit_created, device=None):
        self.journal, self.log, self.result, self.state = journal, log, result, state
        self.audit_created = audit_created
        self.device = device
        self.journal_identity = None
        self.api = ctypes.WinDLL("kernel32", use_last_error=True)
        signatures = {
            "FindFirstChangeNotificationW": ([w.LPCWSTR, w.BOOL, w.DWORD], w.HANDLE),
            "FindNextChangeNotification": ([w.HANDLE], w.BOOL),
            "FindCloseChangeNotification": ([w.HANDLE], w.BOOL),
            "WaitForMultipleObjects": ([w.DWORD, ctypes.POINTER(w.HANDLE), w.BOOL, w.DWORD], w.DWORD),
        }
        for name, (arguments, result_type) in signatures.items():
            function = getattr(self.api, name)
            function.argtypes, function.restype = arguments, result_type
        self.change = self.api.FindFirstChangeNotificationW(str(journal.parent), False, 0x1 | 0x8 | 0x10)
        if self.change in (None, ctypes.c_void_p(-1).value):
            raise ctypes.WinError(ctypes.get_last_error())

    def inspect(self):
        for path, cap in ((self.log, 64 << 20), (self.result, 16 << 20), (self.journal, JOURNAL_BYTES)):
            try:
                metadata = path.lstat()
            except FileNotFoundError:
                continue
            require(not path.is_symlink() and not getattr(metadata, "st_file_attributes", 0) & 0x400
                    and path.is_file() and metadata.st_size <= cap, "unchecked or oversized native output")
            if path == self.journal:
                identity = (metadata.st_dev, metadata.st_ino)
                if self.journal_identity is None:
                    self.audit_created(path)
                    self.journal_identity = identity
                require(identity == self.journal_identity, "native journal file identity changed")
                with path.open("rb") as stream:
                    raw = stream.read(JOURNAL_BYTES + 1)
                self.state.feed(raw, time.monotonic_ns())

    def run(self, api, _job, info, _whole_deadline):
        handles = (w.HANDLE * 2)(self.change, info.process)
        try:
            while True:
                self.inspect()
                now = time.monotonic_ns()
                self.state.check_expiry(now)
                expiry = self.state.expiry()
                if self.device is not None:
                    self.device.observe_if_due(now)
                    expiry = min(expiry, self.device.next_sample_ns)
                now = time.monotonic_ns()
                wait_ms = max(0, (expiry - now + 999_999) // 1_000_000)
                waited = self.api.WaitForMultipleObjects(2, handles, False, wait_ms)
                if waited == 0:
                    if not self.api.FindNextChangeNotification(self.change):
                        raise ctypes.WinError(ctypes.get_last_error())
                elif waited == 1:
                    self.inspect()
                    self.state.check_expiry(time.monotonic_ns())
                    code = w.DWORD()
                    if not api.GetExitCodeProcess(info.process, ctypes.byref(code)):
                        raise ctypes.WinError(ctypes.get_last_error())
                    self.state.prove_exit(code.value)
                    return {"worker_pid": int(info.pid),
                            "operation_watch": {**self.state.proof(),
                                                "actual_process_exit_observed": True,
                                                "exit_code": int(code.value)}}, []
                elif waited != 258:
                    raise ctypes.WinError(ctypes.get_last_error())
        except BaseException as error:
            error.operation_watch = {**self.state.proof(), "failure": str(error)}
            raise

    def close(self):
        if self.change is not None:
            change, self.change = self.change, None
            if not self.api.FindCloseChangeNotification(change):
                raise ctypes.WinError(ctypes.get_last_error())
