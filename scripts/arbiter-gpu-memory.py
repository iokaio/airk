# SPDX-License-Identifier: Apache-2.0
"""Sampled Windows process GPU memory; see arbiter-gpu-memory/CONTRACT.md.

Library only. Import performs no DLL loading, probing, process launch or file IO.
"""
import ctypes
from dataclasses import asdict, dataclass
import math
import os
import platform
import threading
import time

U32 = ctypes.c_uint32
I32 = ctypes.c_int32
U64 = ctypes.c_uint64


class Luid(ctypes.Structure):
    _fields_ = [("LowPart", U32), ("HighPart", I32)]


class OpenAdapter(ctypes.Structure):
    _fields_ = [("AdapterLuid", Luid), ("hAdapter", U32)]


class CloseAdapter(ctypes.Structure):
    _fields_ = [("hAdapter", U32)]


class QueryVideoMemoryInfo32(ctypes.Structure):
    _pack_ = 8
    _fields_ = [("hProcess", U32), ("hAdapter", U32), ("MemorySegmentGroup", U32),
                ("_padding12", U32), ("Budget", U64), ("CurrentUsage", U64),
                ("CurrentReservation", U64), ("AvailableForReservation", U64),
                ("PhysicalAdapterIndex", U32), ("_padding52", U32)]


class QueryVideoMemoryInfo64(ctypes.Structure):
    _pack_ = 8
    _fields_ = [("hProcess", U64), ("hAdapter", U32), ("MemorySegmentGroup", U32),
                ("Budget", U64), ("CurrentUsage", U64), ("CurrentReservation", U64),
                ("AvailableForReservation", U64), ("PhysicalAdapterIndex", U32),
                ("_padding52", U32)]


def _integer(value, minimum, maximum, name):
    if type(value) is not int or not minimum <= value <= maximum:
        raise ValueError("invalid " + name)
    return value


@dataclass(frozen=True)
class ProcessIdentity:
    pid: int
    created_100ns: int

    def __post_init__(self):
        _integer(self.pid, 1, (1 << 32) - 1, "original PID")
        _integer(self.created_100ns, 1, (1 << 64) - 1, "original creation FILETIME")


@dataclass(frozen=True)
class AdapterIdentity:
    luid_low: int
    luid_high: int
    physical_index: int

    def __post_init__(self):
        _integer(self.luid_low, 0, (1 << 32) - 1, "LUID low")
        _integer(self.luid_high, -(1 << 31), (1 << 31) - 1, "LUID high")
        _integer(self.physical_index, 0, (1 << 32) - 1, "physical adapter index")


@dataclass(frozen=True)
class ProcessState:
    identity: ProcessIdentity
    alive: bool


@dataclass(frozen=True)
class _Binding:
    process_handle: int
    process: ProcessIdentity
    adapter: AdapterIdentity
    deadline: float


VALUE_FIELDS = ("budget_bytes", "current_usage_bytes", "current_reservation_bytes",
                "available_for_reservation_bytes")


class WindowsApi:
    """Lazy explicit System32 API binding; never opens a process by PID."""

    def __init__(self):
        if (os.name != "nt" or ctypes.sizeof(ctypes.c_void_p) != 8
                or platform.machine().lower() not in ("amd64", "x86_64")):
            raise OSError("D3DKMT memory observer API requires Windows x64")
        self.gdi = ctypes.WinDLL("gdi32.dll", winmode=0x800, use_last_error=True)
        self.kernel = ctypes.WinDLL("kernel32.dll", winmode=0x800, use_last_error=True)
        for name, argument in (
            ("D3DKMTOpenAdapterFromLuid", OpenAdapter),
            ("D3DKMTQueryVideoMemoryInfo", QueryVideoMemoryInfo64),
            ("D3DKMTCloseAdapter", CloseAdapter),
        ):
            function = getattr(self.gdi, name)
            function.argtypes = [ctypes.POINTER(argument)]
            function.restype = I32
        class FileTime(ctypes.Structure):
            _fields_ = [("low", U32), ("high", U32)]
        self.FileTime = FileTime
        self.kernel.GetProcessId.argtypes = [ctypes.c_void_p]
        self.kernel.GetProcessId.restype = U32
        self.kernel.GetProcessTimes.argtypes = [ctypes.c_void_p] + [ctypes.POINTER(FileTime)] * 4
        self.kernel.GetProcessTimes.restype = ctypes.c_int32
        self.kernel.WaitForSingleObject.argtypes = [ctypes.c_void_p, U32]
        self.kernel.WaitForSingleObject.restype = U32

    def process_state(self, process_handle):
        pid = int(self.kernel.GetProcessId(process_handle))
        if pid == 0:
            raise ctypes.WinError(ctypes.get_last_error())
        times = [self.FileTime() for _ in range(4)]
        if not self.kernel.GetProcessTimes(process_handle, *[ctypes.byref(t) for t in times]):
            raise ctypes.WinError(ctypes.get_last_error())
        waited = int(self.kernel.WaitForSingleObject(process_handle, 0))
        if waited not in (0, 258):
            raise ctypes.WinError(ctypes.get_last_error())
        created = (int(times[0].high) << 32) | int(times[0].low)
        return ProcessState(ProcessIdentity(pid, created), waited == 258)

    def open_adapter(self, identity):
        request = OpenAdapter(Luid(identity.luid_low, identity.luid_high), 0)
        status = int(self.gdi.D3DKMTOpenAdapterFromLuid(ctypes.byref(request)))
        return status, int(request.hAdapter)

    def query(self, process_handle, adapter_handle, physical_index, segment):
        request = QueryVideoMemoryInfo64()
        request.hProcess = process_handle
        request.hAdapter = adapter_handle
        request.MemorySegmentGroup = segment
        request.PhysicalAdapterIndex = physical_index
        status = int(self.gdi.D3DKMTQueryVideoMemoryInfo(ctypes.byref(request)))
        values = None
        if status == 0:
            values = dict(zip(VALUE_FIELDS, (int(request.Budget), int(request.CurrentUsage),
                          int(request.CurrentReservation), int(request.AvailableForReservation))))
        return status, values

    def close_adapter(self, adapter_handle):
        return int(self.gdi.D3DKMTCloseAdapter(ctypes.byref(CloseAdapter(adapter_handle))))


def _failure(error):
    return {"type": type(error).__name__, "message": str(error)}


def _status(value):
    return _integer(value, -(1 << 31), (1 << 32) - 1, "NTSTATUS")


class Sampler:
    """Borrow original child HANDLE; caller schedules and retains every sample."""

    def __init__(self, api, process_handle, process, adapter, deadline, *, clock=time.monotonic):
        _integer(process_handle, 1, (1 << 64) - 2, "retained original process HANDLE")
        if not isinstance(process, ProcessIdentity) or not isinstance(adapter, AdapterIdentity):
            raise ValueError("explicit original process and adapter identities required")
        self._binding = _Binding(process_handle, process, adapter, self._time(deadline))
        self.api, self.clock = api, clock
        self._last_clock = self._time(clock())
        if self.deadline <= self._last_clock:
            raise ValueError("original absolute deadline already exhausted")
        self._lock = threading.Lock()
        self._adapter_handle = None
        self._open_attempted = False
        self._open_status = None
        self._close_result = None
        self._last_start = None
        self._max_gap = None
        self._count = self._valid_count = 0
        self._maximum = {"local": None, "non_local": None}

    @property
    def process_handle(self):
        return self._binding.process_handle

    @property
    def process(self):
        return self._binding.process

    @property
    def adapter(self):
        return self._binding.adapter

    @property
    def deadline(self):
        return self._binding.deadline

    @staticmethod
    def _time(value):
        if type(value) not in (int, float) or not math.isfinite(value) or value < 0:
            raise ValueError("invalid original monotonic timestamp")
        return float(value)

    def _stamp(self):
        observed = self._time(self.clock())
        if observed < self._last_clock:
            raise ValueError("monotonic clock regressed")
        self._last_clock = observed
        return observed

    def _identity(self):
        state = self.api.process_state(self.process_handle)
        if (not isinstance(state, ProcessState) or state.identity != self.process
                or state.alive is not True):
            raise ValueError("original retained process identity/liveness unavailable")
        return asdict(state)

    def sample(self):
        with self._lock:
            record = {"sequence": self._count + 1, "started_at": None, "finished_at": None,
                      "start_gap_seconds": None, "process_before": None, "process_after": None,
                      "adapter_open_status": self._open_status, "segments": {},
                      "valid": False, "errors": []}
            try:
                start = self._stamp()
                record["started_at"] = start
                if self._last_start is not None:
                    gap = start - self._last_start
                    record["start_gap_seconds"] = gap
                    self._max_gap = max(self._max_gap or 0, gap)
                self._last_start = start
                if self._close_result is not None:
                    raise ValueError("observer already closed")
                if start >= self.deadline:
                    raise TimeoutError("original absolute sampling deadline exhausted")
                record["process_before"] = self._identity()
                if not self._open_attempted:
                    self._open_attempted = True
                    status, handle = self.api.open_adapter(self.adapter)
                    self._open_status = _status(status)
                    record["adapter_open_status"] = self._open_status
                    if self._open_status == 0:
                        self._adapter_handle = _integer(handle, 1, (1 << 32) - 1, "adapter handle")
                record["adapter_open_status"] = self._open_status
                if self._adapter_handle is None:
                    raise OSError("exact adapter open failed; no zero-usage substitution")
                for name, segment in (("local", 0), ("non_local", 1)):
                    item = {"started_at": self._stamp(), "finished_at": None,
                            "ntstatus": None, "values": None, "error": None}
                    record["segments"][name] = item
                    try:
                        if item["started_at"] >= self.deadline:
                            raise TimeoutError("original deadline exhausted before query")
                        status, values = self.api.query(self.process_handle, self._adapter_handle,
                                                        self.adapter.physical_index, segment)
                        item["ntstatus"] = _status(status)
                        if status != 0:
                            raise OSError("non-success video-memory NTSTATUS")
                        if not isinstance(values, dict) or set(values) != set(VALUE_FIELDS):
                            raise ValueError("current memory counters unavailable")
                        for field in VALUE_FIELDS:
                            _integer(values[field], 0, (1 << 64) - 1, field)
                        item["values"] = dict(values)
                    except Exception as error:
                        item["error"] = _failure(error)
                        record["errors"].append({"segment": name, **_failure(error)})
                    item["finished_at"] = self._stamp()
                record["process_after"] = self._identity()
            except Exception as error:
                record["errors"].append(_failure(error))
            finally:
                try:
                    record["finished_at"] = self._stamp()
                    if record["finished_at"] >= self.deadline:
                        raise TimeoutError("query returned after original sampling deadline")
                except Exception as error:
                    record["errors"].append(_failure(error))
            record["valid"] = not record["errors"] and len(record["segments"]) == 2
            self._count += 1
            if record["valid"]:
                self._valid_count += 1
                for name, item in record["segments"].items():
                    value = item["values"]["current_usage_bytes"]
                    prior = self._maximum[name]
                    self._maximum[name] = value if prior is None else max(prior, value)
            return record

    def close(self):
        with self._lock:
            if self._close_result is None:
                result = {"adapter_close_attempted": self._adapter_handle is not None,
                          "adapter_close_status": None, "closed": False,
                          "process_handle_closed": False, "error": None}
                try:
                    if self._adapter_handle is not None:
                        result["adapter_close_status"] = _status(self.api.close_adapter(self._adapter_handle))
                        if result["adapter_close_status"] != 0:
                            raise OSError("adapter metadata handle close failed")
                    result["closed"] = True
                except Exception as error:
                    result["error"] = _failure(error)
                self._close_result = result
            return dict(self._close_result)

    def summary(self):
        with self._lock:
            return {"scope": "sampled current OS process GPU memory; sequential segment queries",
                    "process": asdict(self.process), "adapter": asdict(self.adapter),
                    "original_deadline": self.deadline, "sample_count": self._count,
                    "valid_sample_count": self._valid_count,
                    "unknown_sample_count": self._count - self._valid_count,
                    "maximum_start_gap_seconds": self._max_gap,
                    "sampled_max_current_usage_bytes": dict(self._maximum),
                    "lifetime_peak_bytes": None, "admission_authority": False,
                    "final_gpu_qualification": False,
                    "close": None if self._close_result is None else dict(self._close_result)}
