# SPDX-License-Identifier: Apache-2.0
import ctypes
import json
from pathlib import Path
from ctypes import wintypes

BASE = Path(__file__).resolve().parent
FIXTURES = BASE / "fixtures"
FIXTURES.mkdir(exist_ok=False)
api = ctypes.WinDLL("kernel32", use_last_error=True)
api.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, wintypes.LPVOID, wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
api.CreateFileW.restype = wintypes.HANDLE
api.CloseHandle.argtypes = [wintypes.HANDLE]
api.CloseHandle.restype = wintypes.BOOL
api.MoveFileExW.argtypes = [wintypes.LPCWSTR, wintypes.LPCWSTR, wintypes.DWORD]
api.MoveFileExW.restype = wintypes.BOOL
api.DeleteFileW.argtypes = [wintypes.LPCWSTR]
api.DeleteFileW.restype = wintypes.BOOL
INVALID_HANDLE = ctypes.c_void_p(-1).value
rows = []


def fixture(name):
    root = FIXTURES / name / "installation"
    root.mkdir(parents=True)
    image = root / "airk.exe"
    image.write_bytes(b"inert fictional AIRK image, never executed\n")
    (root / "owner.json").write_text('{"fixture":true}\n')
    return root, image


def handle(path, access=0xC0000000, share=1 | 4):
    value = api.CreateFileW(str(path), access, share, None, 3, 0x80, None)
    if value == INVALID_HANDLE:
        raise ctypes.WinError(ctypes.get_last_error())
    return value


def call(name, function, *args):
    ctypes.set_last_error(0)
    success = bool(function(*args))
    rows.append({"case": name, "success": success, "winerror": 0 if success else ctypes.get_last_error()})
    return success


for flags in (0, 1):
    root, image = fixture(f"no-handle-flags-{flags}")
    call(f"directory no handle flags={flags}", api.MoveFileExW, str(root), str(root.with_name("retired")), flags)

for label, access, share, flags in (
    ("write-denywrite-flags0", 0xC0000000, 1 | 4, 0),
    ("write-denywrite-flags1", 0xC0000000, 1 | 4, 1),
    ("readonly-share-delete", 0x80000000, 1 | 4, 0),
    ("write-share-all", 0xC0000000, 1 | 2 | 4, 0),
):
    root, image = fixture(label)
    value = handle(image, access, share)
    try:
        call(f"directory with held {label}", api.MoveFileExW, str(root), str(root.with_name("retired")), flags)
    finally:
        api.CloseHandle(value)

root, image = fixture("unlink-close-directory")
value = handle(image)
try:
    call("unlink image while held write exclusion", api.DeleteFileW, str(image))
    attempt = api.CreateFileW(str(image), 0x80000000, 1 | 2 | 4, None, 3, 0x80, None)
    reopen_error = ctypes.get_last_error() if attempt == INVALID_HANDLE else 0
    if attempt != INVALID_HANDLE:
        api.CloseHandle(attempt)
    rows.append({"case": "open image name after unlink before close", "success": attempt != INVALID_HANDLE, "winerror": reopen_error})
    call("directory after unlink while image still held", api.MoveFileExW, str(root), str(root.with_name("retired")), 0)
finally:
    api.CloseHandle(value)
rows.append({"case": "image absent after held handle closes", "success": not image.exists(), "winerror": 0})
call("directory after unlink and handle close", api.MoveFileExW, str(root), str(root.with_name("retired")), 0)

root, image = fixture("rename-image-outside-parent")
value = handle(image)
outside = root.parent / "retired-image.exe"
try:
    call("rename image outside parent while held", api.MoveFileExW, str(image), str(outside), 0)
    call("rename old parent with held relocated image", api.MoveFileExW, str(root), str(root.with_name("retired")), 0)
finally:
    api.CloseHandle(value)

root, image = fixture("metadata-held")
value = handle(root / "owner.json", 0x80000000, 1 | 2 | 4)
try:
    call("directory with unrelated child metadata handle", api.MoveFileExW, str(root), str(root.with_name("retired")), 0)
finally:
    api.CloseHandle(value)

receipt = {"scope": "fictional workspace-private Windows file handles; no process launched; no user installation accessed", "fixtures": str(FIXTURES), "observations": rows}
(BASE / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
print(json.dumps(receipt, indent=2))
