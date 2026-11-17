# SPDX-License-Identifier: Apache-2.0
"""Isolated Windows MoveFileEx target-handle diagnostic; no AIRK source changes."""
import ctypes
from ctypes import wintypes as w
import json
from pathlib import Path
import tempfile

k = ctypes.WinDLL("kernel32", use_last_error=True)
k.CreateFileW.argtypes = [w.LPCWSTR, w.DWORD, w.DWORD, ctypes.c_void_p, w.DWORD, w.DWORD, w.HANDLE]
k.CreateFileW.restype = w.HANDLE
k.CloseHandle.argtypes = [w.HANDLE]
k.MoveFileExW.argtypes = [w.LPCWSTR, w.LPCWSTR, w.DWORD]
k.MoveFileExW.restype = w.BOOL
rows = []
with tempfile.TemporaryDirectory(prefix="airk-share-probe-") as directory:
    root = Path(directory)
    for access in (0, 0x80000000, 0xC0000000):
        for pinned in (False, True):
            target, editor = root / "target", root / "editor"
            target.write_bytes(b"original")
            editor.write_bytes(b"editor")
            handles = []
            if pinned:
                handles.append(k.CreateFileW(directory, 0x20081, 3, None, 3, 0x02200000, None))
            if access:
                handles.append(k.CreateFileW(str(target), access, 7, None, 3, 0x200000, None))
            try:
                assert all(h != ctypes.c_void_p(-1).value for h in handles)
                moved = k.MoveFileExW(str(editor), str(target), 1)
                error = ctypes.get_last_error() if not moved else 0
                rows.append(dict(target_access=hex(access), parent_pinned=pinned, moved=bool(moved), error=error))
            finally:
                for handle in reversed(handles):
                    k.CloseHandle(handle)
                for item in (target, editor):
                    if item.exists():
                        item.unlink()
result = json.dumps(rows, indent=2)
print(result)
Path(__file__).with_suffix(".json").write_text(result)
