# SPDX-License-Identifier: Apache-2.0
"""Read-only actual Windows owner/DACL identity; never changes an ACL."""
import ctypes,os
from ctypes import wintypes as w
def observe(path):
    if os.name!='nt':raise ValueError('Windows ownership observation required')
    a=ctypes.WinDLL('advapi32',use_last_error=True);k=ctypes.WinDLL('kernel32',use_last_error=True)
    a.GetNamedSecurityInfoW.argtypes=[w.LPWSTR,w.DWORD,w.DWORD,ctypes.POINTER(ctypes.c_void_p),ctypes.c_void_p,ctypes.c_void_p,ctypes.c_void_p,ctypes.POINTER(ctypes.c_void_p)];a.GetNamedSecurityInfoW.restype=w.DWORD
    a.ConvertSidToStringSidW.argtypes=[ctypes.c_void_p,ctypes.POINTER(w.LPWSTR)];a.ConvertSidToStringSidW.restype=w.BOOL
    a.ConvertSecurityDescriptorToStringSecurityDescriptorW.argtypes=[ctypes.c_void_p,w.DWORD,w.DWORD,ctypes.POINTER(w.LPWSTR),ctypes.c_void_p];a.ConvertSecurityDescriptorToStringSecurityDescriptorW.restype=w.BOOL
    k.LocalFree.argtypes=[ctypes.c_void_p];k.LocalFree.restype=ctypes.c_void_p
    owner=ctypes.c_void_p();sd=ctypes.c_void_p();sid=w.LPWSTR();sddl=w.LPWSTR()
    status=a.GetNamedSecurityInfoW(str(path),1,5,ctypes.byref(owner),None,None,None,ctypes.byref(sd))
    if status:raise ctypes.WinError(status)
    try:
        if not a.ConvertSidToStringSidW(owner,ctypes.byref(sid)):raise ctypes.WinError(ctypes.get_last_error())
        if not a.ConvertSecurityDescriptorToStringSecurityDescriptorW(sd,1,5,ctypes.byref(sddl),None):raise ctypes.WinError(ctypes.get_last_error())
        return {'owner_sid':sid.value,'owner_dacl_sddl':sddl.value}
    finally:
        if sid:k.LocalFree(sid)
        if sddl:k.LocalFree(sddl)
        if sd:k.LocalFree(sd)
