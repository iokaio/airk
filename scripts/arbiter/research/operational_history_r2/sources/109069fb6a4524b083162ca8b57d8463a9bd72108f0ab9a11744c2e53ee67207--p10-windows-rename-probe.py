# SPDX-License-Identifier: Apache-2.0
import ctypes as c
from ctypes import wintypes as w
from pathlib import Path
import tempfile

k=c.WinDLL('kernel32',use_last_error=True)
n=c.WinDLL('ntdll')
k.CreateFileW.argtypes=[w.LPCWSTR,w.DWORD,w.DWORD,c.c_void_p,w.DWORD,w.DWORD,w.HANDLE]
k.CreateFileW.restype=w.HANDLE
k.CloseHandle.argtypes=[w.HANDLE]
class IO(c.Structure):
    _fields_=[('status',c.c_void_p),('information',c.c_size_t)]
class Rename(c.Structure):
    _fields_=[('flags',w.DWORD),('root',w.HANDLE),('length',w.DWORD),('name',w.WCHAR*1)]
n.NtSetInformationFile.argtypes=[w.HANDLE,c.POINTER(IO),c.c_void_p,w.DWORD,w.DWORD]
n.NtSetInformationFile.restype=c.c_long
def opened(path,access,dir=False):
    h=k.CreateFileW(str(path),access,7,None,3,0x200000| (0x2000000 if dir else 0),None)
    if h==c.c_void_p(-1).value: raise c.WinError(c.get_last_error())
    return h
with tempfile.TemporaryDirectory(prefix='p10-rename-',dir=Path(__file__).parent) as tmp:
    p=Path(tmp)
    for kind,flags in [(10,1),(65,3)]:
        (p/'source').write_bytes(b'new owned file')
        (p/'target').write_bytes(b'old owned file')
        original=opened(p/'target',0x80000000)
        source=opened(p/'source',0x10000|0x80)
        directory=opened(p,0x20|0x80|2,True)
        try:
            name='target'.encode('utf-16le')
            buf=c.create_string_buffer(c.sizeof(Rename)+len(name))
            value=Rename.from_buffer(buf)
            value.flags=flags; value.root=directory; value.length=len(name)
            c.memmove(c.addressof(buf)+Rename.name.offset,name,len(name))
            io=IO()
            result=n.NtSetInformationFile(source,c.byref(io),buf,len(buf),kind)
            print(f'class={kind}, flags={flags}: NTSTATUS=0x{result&0xffffffff:08x}')
        finally:
            for handle in [original,source,directory]: k.CloseHandle(handle)
        print(f'after handle close: target={(p/"target").read_bytes()!r}')
        for path in [p/'source',p/'target']:
            if path.exists(): path.unlink()
