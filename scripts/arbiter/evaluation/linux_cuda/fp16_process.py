# SPDX-License-Identifier: Apache-2.0
"""Pre-exec custody for one Linux x86_64 FP16 scoring process and its threads."""
import ctypes
import errno
import os
from pathlib import Path
import platform
import resource
import signal
import sys


class Instruction(ctypes.Structure):
    _fields_ = [("code", ctypes.c_ushort), ("jt", ctypes.c_ubyte),
                ("jf", ctypes.c_ubyte), ("k", ctypes.c_uint32)]


class Program(ctypes.Structure):
    _fields_ = [("length", ctypes.c_ushort), ("instructions", ctypes.POINTER(Instruction))]


def bind_process(parent_pid: int, host_limit: int) -> None:
    """Call in the single-threaded direct child before exec of the sealed image."""
    if (platform.system(), platform.machine()) != ("Linux", "x86_64"):
        raise ValueError("requires the managed Linux x86_64 target")
    if os.getuid() == 0 or os.getresuid() != (os.getuid(),) * 3 or host_limit <= 0:
        raise ValueError("requires non-root same-user execution and finite address limit")
    if len(list(Path("/proc/self/task").iterdir())) != 1:
        raise ValueError("install inherited custody before starting threads")
    libc = ctypes.CDLL(None, use_errno=True)

    def prctl(option, a=0, b=0):
        if libc.prctl(option, ctypes.c_ulong(a), ctypes.c_ulong(b),
                      ctypes.c_ulong(0), ctypes.c_ulong(0)) != 0:
            err = ctypes.get_errno()
            raise OSError(err, os.strerror(err))

    prctl(1, signal.SIGKILL)  # PR_SET_PDEATHSIG
    if os.getppid() != parent_pid or parent_pid <= 1:
        raise RuntimeError("supervising parent changed")
    resource.setrlimit(resource.RLIMIT_AS, (host_limit, host_limit))
    if resource.getrlimit(resource.RLIMIT_AS) != (host_limit, host_limit):
        raise RuntimeError("kernel address-space ceiling differs")
    prctl(38, 1)  # PR_SET_NO_NEW_PRIVS
    # linux/uapi/asm-generic/seccomp.h and asm/unistd_64.h. Only NPTL
    # threads sharing this address space and thread group may be created.
    # clone3 carries its flags behind a pointer; ENOSYS selects glibc clone.
    load, eq, bit, mask, ret = 0x20, 0x15, 0x45, 0x54, 0x06
    deny = 0x00050000 | errno.EPERM
    required = 0x100 | 0x800 | 0x10000  # CLONE_VM | SIGHAND | THREAD
    ins = [
        (load, 0, 0, 4), (eq, 1, 0, 0xc000003e), (ret, 0, 0, deny),
        (load, 0, 0, 0), (bit, 0, 1, 0x40000000), (ret, 0, 0, deny),
        (eq, 0, 1, 57), (ret, 0, 0, deny),  # fork
        (eq, 0, 1, 58), (ret, 0, 0, deny),  # vfork
        (eq, 0, 1, 435), (ret, 0, 0, 0x00050000 | errno.ENOSYS),
        (eq, 0, 4, 56), (load, 0, 0, 16), (mask, 0, 0, required),
        (eq, 1, 0, required), (ret, 0, 0, deny),
        (ret, 0, 0, 0x7fff0000),
    ]
    array = (Instruction * len(ins))(*(Instruction(*row) for row in ins))
    program = Program(len(array), array)
    prctl(22, 2, ctypes.addressof(program))  # PR_SET_SECCOMP / FILTER


def process_memory(pid: int) -> dict:
    """Read independent kernel observations. All values are bytes, except Threads."""
    root = Path("/proc") / str(pid)
    observations = {}
    units = {"VmHWM", "VmRSS", "VmSize", "VmPeak", "RssAnon", "RssFile",
             "RssShmem", "Private_Clean", "Private_Dirty", "Swap", "Pss"}
    for filename in ("status", "smaps_rollup"):
        for line in (root / filename).read_text().splitlines():
            key, _, value = line.partition(":")
            if key in units:
                size, unit = value.split()
                if unit != "kB" or key in observations:
                    raise ValueError("unknown or repeated kernel memory field")
                observations[key] = int(size) * 1024
            if key == "Threads":
                observations[key] = int(value.strip())
    if observations.keys() != units | {"Threads"}:
        raise ValueError("missing kernel process memory counters")
    return observations


if __name__ == "__main__":
    # The supervisor passes the held native image as an inherited descriptor.
    # This fresh interpreter stays single-threaded before imposing seccomp.
    parent, address, fd = map(int, sys.argv[1:4])
    bind_process(parent, address)
    os.set_inheritable(fd, False)
    os.execve(fd, sys.argv[4:], dict(os.environ))
