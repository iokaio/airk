# SPDX-License-Identifier: Apache-2.0
"""Bounded Windows private-record read; read-only handle permits atomic rename.

Uses the established check-runtime-activation.py::read_intent CreateFile sharing
pattern, adding native private-record nofollow/type/link/owner/DACL/size/ID guards.
No ACL change, permissions fallback, mutation, native request, or sleep exists.
"""
import ctypes,msvcrt,os,time
from ctypes import wintypes as w

SID='S-1-5-21-3678370870-3858642417-826968105-1002'
K=ctypes.WinDLL('kernel32',use_last_error=True);A=ctypes.WinDLL('advapi32',use_last_error=True)
K.CreateFileW.argtypes=[w.LPCWSTR,w.DWORD,w.DWORD,ctypes.c_void_p,w.DWORD,w.DWORD,w.HANDLE];K.CreateFileW.restype=w.HANDLE
K.CloseHandle.argtypes=[w.HANDLE];K.CloseHandle.restype=w.BOOL
K.LocalFree.argtypes=[ctypes.c_void_p];K.LocalFree.restype=ctypes.c_void_p
K.GetFileType.argtypes=[w.HANDLE];K.GetFileType.restype=w.DWORD
A.GetSecurityInfo.argtypes=[w.HANDLE,w.DWORD,w.DWORD,ctypes.POINTER(ctypes.c_void_p),ctypes.c_void_p,
    ctypes.POINTER(ctypes.c_void_p),ctypes.c_void_p,ctypes.POINTER(ctypes.c_void_p)];A.GetSecurityInfo.restype=w.DWORD
A.ConvertSidToStringSidW.argtypes=[ctypes.c_void_p,ctypes.POINTER(w.LPWSTR)];A.ConvertSidToStringSidW.restype=w.BOOL
A.GetAce.argtypes=[ctypes.c_void_p,w.DWORD,ctypes.POINTER(ctypes.c_void_p)];A.GetAce.restype=w.BOOL
class Info(ctypes.Structure):
    _fields_=[('attributes',w.DWORD),('created',w.FILETIME),('accessed',w.FILETIME),('written',w.FILETIME),
        ('volume',w.DWORD),('size_high',w.DWORD),('size_low',w.DWORD),('links',w.DWORD),('index_high',w.DWORD),('index_low',w.DWORD)]
K.GetFileInformationByHandle.argtypes=[w.HANDLE,ctypes.POINTER(Info)];K.GetFileInformationByHandle.restype=w.BOOL

def require(ok,text):
    if not ok:raise ValueError(text)
def sid_string(pointer):
    text=w.LPWSTR();require(A.ConvertSidToStringSidW(pointer,ctypes.byref(text)),'Private record SID unreadable')
    try:return text.value
    finally:K.LocalFree(ctypes.cast(text,ctypes.c_void_p))
def private_security(handle):
    owner,dacl,descriptor=ctypes.c_void_p(),ctypes.c_void_p(),ctypes.c_void_p()
    code=A.GetSecurityInfo(handle,1,5,ctypes.byref(owner),None,ctypes.byref(dacl),None,ctypes.byref(descriptor))
    if code:raise ctypes.WinError(code)
    try:
        require(owner.value and dacl.value,'Null owner/private DACL refused')
        user=sid_string(owner);require(user==SID,'Foreign private record owner refused')
        count=ctypes.c_ushort.from_address(dacl.value+4).value;require(count>0,'Empty private DACL refused')
        allowed=[]
        for index in range(count):
            ace=ctypes.c_void_p();require(A.GetAce(dacl,index,ctypes.byref(ace)) and ace.value,'Private record ACE unreadable')
            kind=ctypes.c_ubyte.from_address(ace.value).value
            if kind==1:continue
            require(kind==0 and ctypes.c_ushort.from_address(ace.value+2).value>=12,'Unsupported private record ACE refused')
            principal=sid_string(ace.value+8);require(principal in (SID,'S-1-5-18','S-1-5-32-544'),'Private record grants another account access')
            allowed.append(principal)
        return {'owner_sid':user,'allowed_sids':allowed}
    finally:K.LocalFree(descriptor)
def information(handle,maximum):
    item=Info();require(K.GetFileInformationByHandle(handle,ctypes.byref(item)),'Private record metadata unreadable')
    size=(int(item.size_high)<<32)|int(item.size_low)
    require(K.GetFileType(handle)==1 and not item.attributes&(0x400|0x10),'Nonordinary/reparse private record refused')
    require(item.links==1 and 0<size<=maximum,'Private record size/link bound refused')
    return {'volume':int(item.volume),'file_id':(int(item.index_high)<<32)|int(item.index_low),'bytes':size,'links':int(item.links),
        'created':(int(item.created.dwHighDateTime)<<32)|int(item.created.dwLowDateTime),
        'written':(int(item.written.dwHighDateTime)<<32)|int(item.written.dwLowDateTime)}
def read_once(path,maximum=8*1024*1024):
    handle=K.CreateFileW(str(path),0x80000000,7,None,3,0x00200080,None)
    if handle==w.HANDLE(-1).value:raise ctypes.WinError(ctypes.get_last_error())
    transferred=False
    try:
        before=information(handle,maximum);security=private_security(handle)
        descriptor=msvcrt.open_osfhandle(handle,os.O_RDONLY|os.O_BINARY);transferred=True
        with os.fdopen(descriptor,'rb') as stream:
            raw=stream.read(maximum+1);after=information(handle,maximum);after_security=private_security(handle)
            require(before==after and security==after_security and len(raw)==before['bytes'],'Private record changed during read')
        return raw,{'metadata':before,'security':security,'share_flags':7,'nofollow':True,'maximum':maximum}
    finally:
        if not transferred:require(K.CloseHandle(handle),'Private record read HANDLE close failed')
def read_observed(path,deadline,maximum=8*1024*1024,reader=read_once,clock=time.monotonic):
    count=0
    while True:
        if clock()>=deadline:raise TimeoutError('Original private-record observation cutoff expired')
        try:
            raw,proof=reader(path,maximum)
            if clock()>=deadline:raise TimeoutError('Private record retained beyond original cutoff')
            return raw,{**proof,'observed_sharing_contentions':count}
        except OSError as error:
            # No generic EACCES/ACL exception: only authentic Windows sharing/lock codes.
            if getattr(error,'winerror',None) not in (32,33):raise
            count+=1