# SPDX-License-Identifier: Apache-2.0
"""Windows named mutex shared with the maintained static-1024 TRAIN capture owner."""
import ctypes
from ctypes import wintypes
import os

MUTEX_NAME = r"Local\AIRK-20261116-Focus1024-Reuse"
ACQUIRED, ABANDONED, BUSY = 0, 0x80, 0x102


class InstallationMutex:
    def __init__(self, name=MUTEX_NAME):
        if os.name != "nt":
            raise OSError("Windows installation ownership required")
        self.name = name
        self.handle = None
        self.api = ctypes.WinDLL("kernel32", use_last_error=True)
        self.api.CreateMutexW.argtypes = (ctypes.c_void_p,wintypes.BOOL,wintypes.LPCWSTR)
        self.api.CreateMutexW.restype = wintypes.HANDLE
        self.api.WaitForSingleObject.argtypes = (wintypes.HANDLE,wintypes.DWORD)
        self.api.WaitForSingleObject.restype = wintypes.DWORD
        self.api.ReleaseMutex.argtypes = self.api.CloseHandle.argtypes = (wintypes.HANDLE,)
        self.api.ReleaseMutex.restype = self.api.CloseHandle.restype = wintypes.BOOL

    def __enter__(self):
        handle = self.api.CreateMutexW(None,False,self.name)
        if not handle:
            raise ctypes.WinError(ctypes.get_last_error())
        status = self.api.WaitForSingleObject(handle,0)
        if status == ACQUIRED:
            self.handle = handle
            return self
        try:
            if status == ABANDONED:
                if not self.api.ReleaseMutex(handle):
                    raise ctypes.WinError(ctypes.get_last_error())
                raise RuntimeError("abandoned shared installation mutex; verify predecessor settlement")
            if status == BUSY:
                raise BlockingIOError("another process owns the shared installation")
            raise ctypes.WinError(ctypes.get_last_error())
        finally:
            self.api.CloseHandle(handle)

    def __exit__(self, *_):
        handle, self.handle = self.handle, None
        try:
            if not self.api.ReleaseMutex(handle):
                raise ctypes.WinError(ctypes.get_last_error())
        finally:
            if not self.api.CloseHandle(handle):
                raise ctypes.WinError(ctypes.get_last_error())


def require_parent_owns(receipt, original_pid):
    if receipt["name"] != MUTEX_NAME or receipt["owner_pid"] != original_pid or original_pid != os.getppid():
        raise ValueError("original grouped owner PID differs")
    try:
        with InstallationMutex():
            pass
    except BlockingIOError:
        return
    raise ValueError("shared installation mutex is not held by the supervised campaign")
