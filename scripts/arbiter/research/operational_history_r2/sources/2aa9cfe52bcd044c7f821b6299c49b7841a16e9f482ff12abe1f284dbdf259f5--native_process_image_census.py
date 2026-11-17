# SPDX-License-Identifier: Apache-2.0
"""Read-only Windows process image query and same-file validation for exact fixtures."""
import ctypes,ctypes.wintypes as w,os
k=ctypes.WinDLL('kernel32',use_last_error=True)
k.OpenProcess.argtypes=[w.DWORD,w.BOOL,w.DWORD];k.OpenProcess.restype=w.HANDLE
k.CloseHandle.argtypes=[w.HANDLE];k.CloseHandle.restype=w.BOOL
k.QueryFullProcessImageNameW.argtypes=[w.HANDLE,w.DWORD,w.LPWSTR,ctypes.POINTER(w.DWORD)];k.QueryFullProcessImageNameW.restype=w.BOOL
k.K32EnumProcesses.argtypes=[ctypes.POINTER(w.DWORD),w.DWORD,ctypes.POINTER(w.DWORD)];k.K32EnumProcesses.restype=w.BOOL
k.CreateFileW.argtypes=[w.LPCWSTR,w.DWORD,w.DWORD,w.LPVOID,w.DWORD,w.DWORD,w.HANDLE];k.CreateFileW.restype=w.HANDLE
class Info(ctypes.Structure):
    _fields_=[(name,w.DWORD)for name in ['attributes','creation_low','creation_high','access_low','access_high','write_low','write_high','volume','size_high','size_low','links','index_high','index_low']]
k.GetFileInformationByHandle.argtypes=[w.HANDLE,ctypes.POINTER(Info)];k.GetFileInformationByHandle.restype=w.BOOL
def canonical(path):
    path=os.fspath(path)
    if path.startswith('\\\\?\\UNC\\'):path='\\\\'+path[8:]
    elif path.startswith('\\\\?\\'):path=path[4:]
    return os.path.normcase(os.path.abspath(path))
def file_id(path):
    handle=k.CreateFileW('\\\\?\\'+os.path.abspath(path) if not str(path).startswith('\\\\?\\') else str(path),0,7,None,3,0,None)
    if handle==ctypes.c_void_p(-1).value:raise ctypes.WinError(ctypes.get_last_error())
    try:
        info=Info()
        if not k.GetFileInformationByHandle(handle,ctypes.byref(info)):raise ctypes.WinError(ctypes.get_last_error())
        return [info.volume,info.index_high,info.index_low]
    finally:k.CloseHandle(handle)
def census(paths):
    assert 0<len(paths)<=96
    selected={canonical(path):os.fspath(path)for path in paths}
    native_ids={key:file_id(value)for key,value in selected.items() if os.path.isfile(value)}
    ids=(w.DWORD*65536)();used=w.DWORD()
    if not k.K32EnumProcesses(ids,ctypes.sizeof(ids),ctypes.byref(used)):raise ctypes.WinError(ctypes.get_last_error())
    assert used.value<ctypes.sizeof(ids),'Process census limit exhausted'
    inaccessible=0;live=[]
    for pid in ids[:used.value//ctypes.sizeof(w.DWORD)]:
        handle=k.OpenProcess(0x1000,False,pid)
        if not handle:inaccessible+=1;continue
        try:
            buffer=ctypes.create_unicode_buffer(32768);length=w.DWORD(len(buffer))
            if not k.QueryFullProcessImageNameW(handle,0,buffer,ctypes.byref(length)):
                inaccessible+=1;continue
            key=canonical(buffer.value)
            if key in selected:
                observed=file_id(buffer.value)
                assert key in native_ids and observed==native_ids[key],'Observed image path no longer matches exact owned file'
                live.append({'pid':pid,'native_path':buffer.value,'owned_path':selected[key],'native_file_id':observed})
        finally:k.CloseHandle(handle)
    return {'live':live,'live_count':len(live),'inaccessible_processes':inaccessible,
            'scope':'native full process image paths restricted to exact canonical fixture paths, verified same volume/file ID; inaccessible processes not certified'}
