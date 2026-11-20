# SPDX-License-Identifier: Apache-2.0
"""Private CUDA Driver API worker. Its owning Windows Job starts and releases ballast."""

import argparse
import ctypes as ct
import hashlib
import json
from pathlib import Path
import sys
import runpy
import time
import uuid

MIB = 1 << 20
GIB = 1 << 30
TIERS = (6, 8, 12, 16, 24)
WINDOW = 128 * MIB
CHUNK = 256 * MIB
SMALL = 64 * MIB
UUID = "08e2b3f6-6a39-3456-77cb-a57287a702ec"
SYSTEM = Path("C:/Windows/System32")
DRIVER = SYSTEM / "nvcuda.dll"
IMPLEMENTATION = SYSTEM / "DriverStore/FileRepository/nv_dispwi.inf_amd64_b9ead98da9a85944/nvcuda64.dll"
PINS = {DRIVER: "f654977a2d34156c861a1434bbbbd1e9173b8d28b7e1c3f415bebca404df42be",
        IMPLEMENTATION: "4cf09882e346099361d9bc1e125d34f076d36bdcf5838da88132e74d1af3d5c6",
        SYSTEM / "nvidia-smi.exe": "a62b9a69d25dc66cdd4d5133ae70ebf079e7ed314ea7a96b91712ec2059ea802",
        Path("C:/Users/tdotn/AppData/Local/Programs/Python/Python313/python.exe"):
            "d932e5e2f324d57f392e8fd063dcf6d0185be8a664c57c6d24e7762ed02c28ca",
        Path("C:/Users/tdotn/AppData/Local/Programs/Python/Python313/python313.dll"):
            "41ec4fc4e5bc8b207258590238f7f050b13fdfee84c4a7b1ecff6e945d031a10"}


def require(ok, message):
    if not ok:
        raise ValueError(message)


def verify(path, expected):
    with path.open("rb") as stream:
        require(hashlib.file_digest(stream, "sha256").hexdigest() == expected, f"changed {path.name}")


def initial_allocation(free, tier, maximum):
    require(tier in TIERS and type(free) is int and 0 < maximum <= (48 - tier) * GIB,
            "invalid resident allocation limit")
    goal = tier * GIB + WINDOW // 2
    size = (free - goal) // MIB * MIB
    require(0 < size <= maximum, "available bytes outside reviewed ballast envelope")
    return size


def read_pressure_grant(raw, nonce, sequence, touched, now):
    raw.seek(0)
    grant = json.loads(raw.read(4096))
    require(grant["nonce"] == nonce and grant["sequence"] == sequence and grant["touched"] == touched
            and 0 <= now-grant["sample_start_ns"] <= 3_000_000_000,
            "stale or mismatched pressure sizing")
    return grant


def pressure_step(free, tier, touched, maximum):
    """Size one bounded increment from contemporaneous independent estimates."""
    require(tier in TIERS and 0 <= touched <= maximum <= (48-tier)*GIB
            and len(free) == 3 and all(type(n) is int and n >= tier*GIB for n in free),
            "observed floor or ballast limit lost")
    cuda, nvidia, wddm = free
    if min(nvidia, wddm) <= tier*GIB + WINDOW:
        require(touched > 0, "initial pressure floor already occupied")
        return 0
    excess = min(cuda, nvidia, wddm) - (tier*GIB + WINDOW//2)
    amount = min(2*GIB, excess//MIB*MIB, maximum-touched)
    require(amount > 0, "no bounded increment reaches the independent tier window")
    return amount


def bind(lib, name, ret, *args):
    fn = getattr(lib, name)
    fn.restype, fn.argtypes = ret, args
    return fn


class CudaError(RuntimeError):
    def __init__(self, operation, status):
        self.operation, self.status = operation, status
        super().__init__(f"{operation} returned CUDA status {status}")


def call(lib, name, types, *args):
    rc = bind(lib, name, ct.c_int, *types)(*args)
    if rc:
        raise CudaError(name, rc)


def loaded_path(kernel, name):
    handle = bind(kernel, "GetModuleHandleW", ct.c_void_p, ct.c_wchar_p)(name)
    require(handle, f"module not loaded: {name}")
    text = ct.create_unicode_buffer(32768)
    n = bind(kernel, "GetModuleFileNameW", ct.c_uint, ct.c_void_p, ct.c_wchar_p, ct.c_uint)(
        handle, text, len(text))
    require(0 < n < len(text), "module path unavailable")
    return Path(text.value)


def emit(kind, **fields):
    print(json.dumps(dict(kind=kind, monotonic_ns=time.monotonic_ns(), **fields), allow_nan=False), flush=True)


def remaining(deadline):
    value = (deadline - time.monotonic_ns()) // 1_000_000
    require(0 < value <= 360000, "original ballast deadline exhausted")
    return value


def run(args):
    require(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode and
            len(PINS) == 5 and Path(sys.executable) in PINS, "reviewed isolated Windows Python required")
    for path, sha in PINS.items():
        verify(path, sha)
    calibration = args.commit_calibration
    require(not calibration or (args.small and args.maximum == 2*GIB), "model-free 2 GiB calibration required")
    pressure_reserve = args.pressure_host_reserve
    require(pressure_reserve is None or (not args.small and not calibration and pressure_reserve >= 16*GIB),
            "original pressure trial and candidate host reserve required")
    host_limit = 4*GIB if calibration else args.maximum + 2*GIB
    accounting = runpy.run_path(str(Path(__file__).with_name("ballast_host_commit.py"))) if calibration or pressure_reserve is not None else None
    def host_sample():
        return {"host":accounting["worker"](host_limit)} if accounting else {}
    remaining(args.deadline_ns)
    require(args.tier in TIERS and 0 < args.maximum <= (48 - args.tier) * GIB and
            (not args.small or args.maximum == (2*GIB if calibration else SMALL)), "unreviewed ballast bounds")
    nonce = str(uuid.UUID(args.nonce))
    kernel = ct.WinDLL(str(SYSTEM / "kernel32.dll"), use_last_error=True)
    dll = ct.WinDLL(str(DRIVER), winmode=0x800)
    integer, uint, size = ct.c_int, ct.c_uint, ct.c_size_t
    ptr, context = ct.c_uint64, ct.c_void_p()
    p = ct.POINTER
    events, allocations = [], []
    device = None
    allocated = touched = freed = 0
    try:
        for operation in ("go", "stop"):
            name = "Local\\airk-ballast-" + nonce + "-" + operation
            handle = bind(kernel, "OpenEventW", ct.c_void_p, uint, integer, ct.c_wchar_p)(0x00100000, 0, name)
            require(handle, "original controller event unavailable")
            events.append(handle)
        wait = bind(kernel, "WaitForSingleObject", uint, ct.c_void_p, uint)
        call(dll, "cuInit", [uint], 0)
        require(loaded_path(kernel, "nvcuda.dll") == DRIVER and
                loaded_path(kernel, "nvcuda64.dll") == IMPLEMENTATION and
                loaded_path(kernel, "python313.dll") == Path(sys.executable).with_name("python313.dll"),
                "loaded CUDA or Python module path differs")
        count, version = integer(), integer()
        call(dll, "cuDeviceGetCount", [p(integer)], ct.byref(count))
        call(dll, "cuDriverGetVersion", [p(integer)], ct.byref(version))
        require(0 < count.value <= 16, "device census unavailable")
        matches = []
        for i in range(count.value):
            ordinal, identifier = integer(), (ct.c_ubyte * 16)()
            call(dll, "cuDeviceGet", [p(integer), integer], ct.byref(ordinal), i)
            call(dll, "cuDeviceGetUuid_v2", [ct.c_void_p, integer], identifier, ordinal)
            if str(uuid.UUID(bytes=bytes(identifier))) == UUID:
                matches.append(ordinal)
        device, = matches
        raw_luid, mask = (ct.c_ubyte * 8)(), uint()
        call(dll, "cuDeviceGetLuid", [ct.c_void_p, p(uint), integer], raw_luid, ct.byref(mask), device)
        lo, hi = (int.from_bytes(bytes(raw_luid)[i:i+4], "little") for i in (0, 4))
        require(f"luid_0x{hi:08x}_0x{lo:08x}" == args.luid and mask.value == 1, "CUDA device/LUID differs")
        call(dll, "cuDevicePrimaryCtxRetain", [p(ct.c_void_p), integer], ct.byref(context), device)
        call(dll, "cuCtxSetCurrent", [ct.c_void_p], context)
        def memory():
            free, total = size(), size()
            call(dll, "cuMemGetInfo_v2", [p(size), p(size)], ct.byref(free), ct.byref(total))
            return free.value, total.value
        free, total = memory()
        emit("context", uuid=UUID, luid=args.luid, driver_api=version.value, free=free, total=total, **host_sample())
        require(wait(events[0], remaining(args.deadline_ns)) == 0 and wait(events[1], 0) == 258,
                "allocation gate unavailable")
        free, total = memory()
        target = args.maximum if args.small or pressure_reserve is not None else initial_allocation(free, args.tier, args.maximum)
        batch_end = sequence = 0
        touched = 0
        while touched < target or pressure_reserve is not None:
            remaining(args.deadline_ns)
            if pressure_reserve is not None and touched == batch_end:
                sequence += 1
                require(sequence <= 64, "pressure sizing round limit reached")
                free, _ = memory()
                emit("sizing", sequence=sequence, touched=touched, free=free)
                require(wait(events[0], remaining(args.deadline_ns)) == 0 and wait(events[1], 0) == 258,
                        "pressure sizing not authorized")
                grant = read_pressure_grant(sys.stdin.buffer.raw, nonce, sequence, touched, time.monotonic_ns())
                free, _ = memory()
                observed = (free, grant["nvidia"], grant["wddm"])
                increment = pressure_step(observed, args.tier, touched, args.maximum)
                emit("sized", sequence=sequence, touched=touched, free=free, increment=increment)
                if increment == 0:
                    break
                batch_end = touched + increment
            require(wait(events[1], 0) == 258, "ballast controller stopped allocation")
            amount = min(CHUNK, (batch_end if pressure_reserve is not None else target) - touched)
            available, _ = memory()
            require(available - amount >= (6 if args.small else args.tier) * GIB,
                    "insufficient measured headroom for next allocation")
            if accounting:
                remaining_host = target - touched
                if pressure_reserve is not None:
                    remaining_host += pressure_reserve + 2*GIB
                accounting["reserve"](accounting["host"](), remaining_host)
            address = ptr()
            try:
                call(dll, "cuMemAlloc_v2", [p(ptr), size], ct.byref(address), amount)
            except CudaError as error:
                emit("allocation_refusal", cuda_status=error.status, requested_bytes=amount,
                     free_before=available, target_bytes=target, touched_bytes=touched,
                     allocated_bytes=allocated, allocated_chunks=len(allocations))
                raise
            allocations.append(address)
            allocated += amount
            call(dll, "cuMemsetD8_v2", [ptr, ct.c_ubyte, size], address, 0xa5, amount)
            call(dll, "cuCtxSynchronize", [])
            touched += amount
            emit("chunk", allocated_bytes=allocated, touched_bytes=touched, chunks=len(allocations), **host_sample())
        free, total = memory()
        emit("held", touched=touched, chunks=len(allocations), free=free, total=total, **host_sample())
        require(wait(events[1], remaining(args.deadline_ns)) == 0, "original hold expired")
    finally:
        try:
            for address in reversed(allocations):
                call(dll, "cuMemFree_v2", [ptr], address)
                freed += 1
            if context.value:
                call(dll, "cuCtxSynchronize", [])
                call(dll, "cuCtxSetCurrent", [ct.c_void_p], None)
                call(dll, "cuDevicePrimaryCtxRelease_v2", [integer], device)
        finally:
            for event in events:
                require(bind(kernel, "CloseHandle", integer, ct.c_void_p)(event), "event close failed")
        emit("released", allocated_bytes=allocated, touched_bytes=touched, freed_chunks=freed,
             allocated_chunks=len(allocations), **host_sample())


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tier", type=int, required=True)
    parser.add_argument("--maximum", type=int, required=True)
    parser.add_argument("--deadline-ns", type=int, required=True)
    parser.add_argument("--luid", required=True)
    parser.add_argument("--nonce", required=True)
    parser.add_argument("--commit-calibration", action="store_true")
    parser.add_argument("--pressure-host-reserve", type=int)
    parser.add_argument("--small", action="store_true")
    try:
        run(parser.parse_args())
    except Exception as error:
        emit("worker_failure", error_type=type(error).__name__, message=str(error),
             cuda_status=getattr(error,"status",None), cuda_operation=getattr(error,"operation",None))
        raise SystemExit(1)
