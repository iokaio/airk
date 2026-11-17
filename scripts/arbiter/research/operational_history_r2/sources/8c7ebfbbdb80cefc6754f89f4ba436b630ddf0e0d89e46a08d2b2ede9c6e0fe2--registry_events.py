# SPDX-License-Identifier: Apache-2.0
"""Windows change event is only a wakeup; native Job reads remain authority."""
import ctypes
from ctypes import wintypes
import os
import time

class RegistryEvents:
    def __init__(self, directory):
        if os.name != 'nt':
            raise ValueError('this actual Windows observer has no other-host qualification')
        self.api = ctypes.WinDLL('kernel32', use_last_error=True)
        self.api.FindFirstChangeNotificationW.argtypes = [wintypes.LPCWSTR, wintypes.BOOL, wintypes.DWORD]
        self.api.FindFirstChangeNotificationW.restype = wintypes.HANDLE
        self.api.FindNextChangeNotification.argtypes = [wintypes.HANDLE]
        self.api.FindNextChangeNotification.restype = wintypes.BOOL
        self.api.FindCloseChangeNotification.argtypes = [wintypes.HANDLE]
        self.api.FindCloseChangeNotification.restype = wintypes.BOOL
        self.api.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
        self.api.WaitForSingleObject.restype = wintypes.DWORD
        self.handle = self.api.FindFirstChangeNotificationW(str(directory), False, 1 | 8 | 16)
        if self.handle == ctypes.c_void_p(-1).value:
            raise ctypes.WinError(ctypes.get_last_error())

    def wait(self, deadline_ns):
        remaining = deadline_ns - time.monotonic_ns()
        if remaining <= 0:
            raise TimeoutError('original setup deadline expired before native change event')
        milliseconds = max(1, min(0xFFFFFFFE, (remaining + 999999) // 1000000))
        result = self.api.WaitForSingleObject(self.handle, milliseconds)
        if result == 258:
            raise TimeoutError('original setup deadline expired waiting on native change')
        if result != 0:
            raise ctypes.WinError(ctypes.get_last_error())
        if not self.api.FindNextChangeNotification(self.handle):
            raise ctypes.WinError(ctypes.get_last_error())

    def close(self):
        if self.handle is not None:
            handle, self.handle = self.handle, None
            if not self.api.FindCloseChangeNotification(handle):
                raise ctypes.WinError(ctypes.get_last_error())
