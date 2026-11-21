# SPDX-License-Identifier: Apache-2.0
"""Measure NTFS file data allocations once per file identity without following reparse points."""
import ctypes
from ctypes import wintypes
import os


class StandardInfo(ctypes.Structure):
    _fields_ = [("allocation", ctypes.c_longlong), ("eof", ctypes.c_longlong),
                ("links", wintypes.DWORD), ("delete_pending", ctypes.c_ubyte), ("directory", ctypes.c_ubyte)]


class FileIdInfo(ctypes.Structure):
    _fields_ = [("volume", ctypes.c_ulonglong), ("identifier", ctypes.c_ubyte * 16)]


def allocation(roots):
    api = ctypes.WinDLL("kernel32", use_last_error=True)
    api.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p,
                                wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
    api.CreateFileW.restype = wintypes.HANDLE
    api.GetFileInformationByHandleEx.argtypes = [wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.DWORD]
    api.GetFileInformationByHandleEx.restype = wintypes.BOOL
    api.CloseHandle.argtypes = [wintypes.HANDLE]
    api.CloseHandle.restype = wintypes.BOOL
    seen = set()
    report = {}
    for name, root in roots.items():
        pending = [root]
        files = size = 0
        while pending:
            with os.scandir(pending.pop()) as entries:
                for entry in entries:
                    if entry.is_dir(follow_symlinks=False) and not entry.is_junction():
                        pending.append(entry.path)
                        continue
                    # Opening the reparse point itself keeps the measurement inside its root.
                    handle = api.CreateFileW(entry.path, 0x80, 7, None, 3, 0x02200000, None)
                    if handle == ctypes.c_void_p(-1).value:
                        raise OSError(ctypes.get_last_error(), "native allocation measurement failed", entry.path)
                    try:
                        standard, identity = StandardInfo(), FileIdInfo()
                        for kind, info in ((1, standard), (18, identity)):
                            if not api.GetFileInformationByHandleEx(handle, kind, ctypes.byref(info), ctypes.sizeof(info)):
                                raise OSError(ctypes.get_last_error(), "native allocation measurement failed", entry.path)
                        key = (identity.volume, bytes(identity.identifier))
                        if key not in seen:
                            seen.add(key)
                            files += 1
                            size += standard.allocation
                    finally:
                        api.CloseHandle(handle)
        report[name] = {"distinct_file_objects": files, "allocated_file_data_bytes": size}
    return report
