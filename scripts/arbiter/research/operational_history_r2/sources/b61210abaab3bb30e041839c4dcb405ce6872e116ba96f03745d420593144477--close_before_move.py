# SPDX-License-Identifier: Apache-2.0
import ctypes
import json
from ctypes import wintypes
from pathlib import Path

base = Path(__file__).resolve().parent
root = base / "fixtures" / "independent-unlink-close-before-move" / "installation"
root.mkdir(parents=True, exist_ok=False)
image = root / "airk.exe"
image.write_bytes(b"inert, never executed\n")
api = ctypes.WinDLL("kernel32", use_last_error=True)
api.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, wintypes.LPVOID, wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
api.CreateFileW.restype = wintypes.HANDLE
api.DeleteFileW.argtypes = [wintypes.LPCWSTR]
api.DeleteFileW.restype = wintypes.BOOL
api.CloseHandle.argtypes = [wintypes.HANDLE]
api.CloseHandle.restype = wintypes.BOOL
api.MoveFileExW.argtypes = [wintypes.LPCWSTR, wintypes.LPCWSTR, wintypes.DWORD]
api.MoveFileExW.restype = wintypes.BOOL
held = api.CreateFileW(str(image), 0xC0000000, 1 | 4, None, 3, 0x80, None)
assert held != ctypes.c_void_p(-1).value, ctypes.get_last_error()
try:
    removed = bool(api.DeleteFileW(str(image)))
    remove_error = 0 if removed else ctypes.get_last_error()
finally:
    closed = bool(api.CloseHandle(held))
    close_error = 0 if closed else ctypes.get_last_error()
moved = bool(api.MoveFileExW(str(root), str(root.with_name("retired")), 0))
result = {"case": "independent exact-image unlink under exclusion, close, exclusive parent move", "unlink": removed, "unlink_error": remove_error, "close": closed, "close_error": close_error, "directory_move": moved, "move_error": 0 if moved else ctypes.get_last_error(), "image_absent": not image.exists() and not root.with_name("retired").joinpath("airk.exe").exists()}
(base / "close-before-move-receipt.json").write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps(result, indent=2))
