# SPDX-License-Identifier: Apache-2.0
"""Query publicly named NVIDIA DRS settings on the exact scientific executable, read-only."""

import ctypes as C
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
IMAGE = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z/windows-cu15-scientific-provider-stage-r1/fresh-cu15-focus1024-protected-r2-inference.exe"
LIBRARY = Path("C:/Windows/System32/nvapi64.dll")
PINS = {IMAGE:"1ddf649e68638400ec2135267f908214c21873c626c3bd4e620ee1446031b33c",
        LIBRARY:"08ead220e3c0d1deda35473e7fc24614cf7fcdffec4941213514440312da1750"}
U32 = C.c_uint32
HANDLE = C.c_void_p
UNICODE = C.c_wchar * 2048


class Application(C.Structure):
    _fields_ = [("version",U32),("predefined",U32),("name",UNICODE),("friendly",UNICODE),("launcher",UNICODE)]


class Setting(C.Structure):
    _pack_ = 4
    _fields_ = [("version",U32),("name",UNICODE),("id",U32),("kind",U32),("location",U32),
                ("predefined",U32),("predefined_valid",U32),("default_data",U32*1025),("current_data",U32*1025)]


def query():
    if sys.platform != "win32" or C.sizeof(C.c_wchar) != 2:
        raise ValueError("Windows Unicode DRS ABI required")
    for path,digest in PINS.items():
        with path.open("rb") as source:
            if hashlib.file_digest(source,"sha256").hexdigest() != digest:
                raise ValueError(f"changed input {path.name}")
    library = C.WinDLL(str(LIBRARY),winmode=0x800)
    lookup = library.nvapi_QueryInterface
    lookup.argtypes, lookup.restype = [U32], HANDLE

    def method(identifier,*types):
        address = lookup(identifier)
        if not address:
            raise ValueError(f"unavailable public NVAPI 0x{identifier:08x}")
        return C.CFUNCTYPE(C.c_int,*types)(address)

    def checked(code):
        if code:
            raise ValueError(f"NVAPI status {code}")

    checked(method(0x0150e828)())
    session = HANDLE()
    checked(method(0x0694d52e,C.POINTER(HANDLE))(C.byref(session)))
    result = {"driver":PINS[LIBRARY],"image":str(IMAGE),"sha256":PINS[IMAGE],"settings":[]}
    try:
        checked(method(0x375dbd6b,HANDLE)(session))
        profile = HANDLE()
        app = Application()
        app.version = C.sizeof(app) | 0x10000
        find = method(0xeee566b2,HANDLE,C.c_wchar_p,C.POINTER(HANDLE),C.POINTER(Application))
        result["find_application_status"] = find(session,str(IMAGE),C.byref(profile),C.byref(app))
        result["application_name"] = app.name
        if result["find_application_status"] == -166:
            checked(method(0x617bff9f,HANDLE,C.POINTER(HANDLE))(session,C.byref(profile)))
            result["profile_source"] = "current-global-no-application-override"
        else:
            checked(result["find_application_status"])
            result["profile_source"] = "application"
        count = U32(8192)
        ids = (U32*count.value)()
        checked(method(0xf020614a,C.POINTER(U32),C.POINTER(U32))(ids,C.byref(count)))
        if count.value > len(ids):
            raise ValueError("DRS public setting census exceeds bound")
        result["available_setting_count"] = count.value
        classes = {}
        lookup_name = method(0xd61cbe6e,U32,C.POINTER(C.c_wchar))
        for setting_id in ids[:count.value]:
            name = UNICODE()
            checked(lookup_name(setting_id,name))
            if "fallback" in name.value.lower() or "sysmem" in name.value.lower():
                classes[setting_id] = name.value
        official_name = "CUDA - Sysmem Fallback Policy"
        setting_id = U32()
        status = method(0xcb7309cd,C.c_wchar_p,C.POINTER(U32))(official_name,C.byref(setting_id))
        result["official_name_lookup_status"] = status
        if status == 0:
            classes[setting_id.value] = official_name
        get = method(0x73bf8338,HANDLE,HANDLE,U32,C.POINTER(Setting))
        for identifier,name in classes.items():
            setting = Setting()
            setting.version = C.sizeof(setting) | 0x10000
            code = get(session,profile,identifier,C.byref(setting))
            result["settings"].append({"id":hex(identifier),"public_name":name,"status":code,
                "reported_name":setting.name,"location":setting.location if not code else None,
                "dword":setting.current_data[0] if code == 0 and setting.kind == 0 else None})
        return result
    finally:
        checked(method(0xdad9cff8,HANDLE)(session))


if __name__ == "__main__":
    print(json.dumps(query(),indent=2))
