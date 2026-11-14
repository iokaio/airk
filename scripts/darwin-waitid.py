# SPDX-License-Identifier: Apache-2.0
"""Darwin64 waitid WNOWAIT for Python builds that omit os.waitid.

ABI checked against Mac Neo's actual SDK sys/{signal,wait}.h with syntax-only
static assertions: siginfo104B, code8/pid12/status20; P_PID1, WEXITED4,
WNOHANG1, WNOWAIT32. This observes, never reaps, the exact owned leader.
"""
import ctypes
import os
import sys
from types import SimpleNamespace


class Siginfo(ctypes.Structure):
    _fields_ = [("si_signo", ctypes.c_int32), ("si_errno", ctypes.c_int32),
                ("si_code", ctypes.c_int32), ("si_pid", ctypes.c_int32),
                ("si_uid", ctypes.c_uint32), ("si_status", ctypes.c_int32),
                ("si_addr", ctypes.c_uint64), ("si_value", ctypes.c_uint64),
                ("si_band", ctypes.c_int64), ("reserved", ctypes.c_uint64 * 7)]


def decode(info, expected_pid):
    if info.si_pid == 0:
        return None
    if info.si_pid != expected_pid or info.si_code not in (1, 2, 3) or not 0 <= info.si_status <= 255:
        raise ValueError("Darwin owned leader observation identity/status differs")
    return SimpleNamespace(si_pid=info.si_pid, si_code=info.si_code, si_status=info.si_status)


def observer():
    if (sys.platform != "darwin" or ctypes.sizeof(ctypes.c_void_p) != 8
            or ctypes.sizeof(ctypes.c_long) != 8 or ctypes.sizeof(Siginfo) != 104
            or Siginfo.si_code.offset != 8 or Siginfo.si_pid.offset != 12 or Siginfo.si_status.offset != 20):
        raise ValueError("Unqualified Darwin waitid ABI")
    library = ctypes.CDLL("/usr/lib/libSystem.B.dylib", use_errno=True)
    function = library.waitid
    function.argtypes = [ctypes.c_int, ctypes.c_uint32, ctypes.POINTER(Siginfo), ctypes.c_int]
    function.restype = ctypes.c_int

    def observed(pid):
        if not isinstance(pid, int) or not 0 < pid <= 0x7fffffff:
            raise ValueError("Invalid actual owned leader PID")
        info = Siginfo()
        if function(1, pid, ctypes.byref(info), 4 | 1 | 32) != 0:
            error = ctypes.get_errno()
            raise OSError(error, os.strerror(error))
        return decode(info, pid)
    # The callback retains both the C entry point and the actual loaded system
    # library until every observation/owned-group cleanup is complete.
    observed.native_library = library
    return observed
