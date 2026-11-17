# SPDX-License-Identifier: Apache-2.0
"""Windows current-SID-only prospective training output admission; no mutations/process starts."""
from pathlib import Path
import os,ctypes
from ctypes import wintypes as w
FULL_CONTROL=0x1f01ff

def validate_acl_observation(row):
 if row['owner_sid']!=row['current_sid'] or row['protected'] is not True or len(row['aces'])!=1:raise ValueError('private anchor must have protected current-SID-only ownership and DACL')
 ace=row['aces'][0]
 if ace['type']!=0 or ace['sid']!=row['current_sid'] or ace['mask']&FULL_CONTROL!=FULL_CONTROL or ace['flags']&0x1c or ace['flags']&3!=3:raise ValueError('private anchor requires sole current-SID inheritable FullControl')
 return row

def private_acl(path):
 if os.name!='nt':raise ValueError('reviewed Windows private training host required')
 api=ctypes.WinDLL('advapi32',use_last_error=True);kernel=ctypes.WinDLL('kernel32',use_last_error=True)
 kernel.GetCurrentProcess.restype=w.HANDLE;kernel.CloseHandle.argtypes=[w.HANDLE];kernel.CloseHandle.restype=w.BOOL;kernel.LocalFree.argtypes=[ctypes.c_void_p];kernel.LocalFree.restype=ctypes.c_void_p
 api.OpenProcessToken.argtypes=[w.HANDLE,w.DWORD,ctypes.POINTER(w.HANDLE)];api.OpenProcessToken.restype=w.BOOL
 api.GetTokenInformation.argtypes=[w.HANDLE,ctypes.c_int,ctypes.c_void_p,w.DWORD,ctypes.POINTER(w.DWORD)];api.GetTokenInformation.restype=w.BOOL
 api.GetNamedSecurityInfoW.argtypes=[w.LPWSTR,ctypes.c_int,w.DWORD,ctypes.POINTER(ctypes.c_void_p),ctypes.c_void_p,ctypes.POINTER(ctypes.c_void_p),ctypes.c_void_p,ctypes.POINTER(ctypes.c_void_p)];api.GetNamedSecurityInfoW.restype=w.DWORD
 api.GetSecurityDescriptorControl.argtypes=[ctypes.c_void_p,ctypes.POINTER(w.WORD),ctypes.POINTER(w.DWORD)];api.GetSecurityDescriptorControl.restype=w.BOOL
 api.GetAce.argtypes=[ctypes.c_void_p,w.DWORD,ctypes.POINTER(ctypes.c_void_p)];api.GetAce.restype=w.BOOL
 api.ConvertSidToStringSidW.argtypes=[ctypes.c_void_p,ctypes.POINTER(w.LPWSTR)];api.ConvertSidToStringSidW.restype=w.BOOL
 def checked(ok):
  if not ok:raise ctypes.WinError(ctypes.get_last_error())
 def sid_text(sid):
  text=w.LPWSTR();checked(api.ConvertSidToStringSidW(sid,ctypes.byref(text)))
  try:return text.value
  finally:kernel.LocalFree(ctypes.cast(text,ctypes.c_void_p))
 class User(ctypes.Structure):_fields_=[('sid',ctypes.c_void_p),('attributes',w.DWORD)]
 class ACL(ctypes.Structure):_fields_=[('revision',w.BYTE),('sbz1',w.BYTE),('size',w.WORD),('count',w.WORD),('sbz2',w.WORD)]
 class ACE(ctypes.Structure):_fields_=[('type',w.BYTE),('flags',w.BYTE),('size',w.WORD),('mask',w.DWORD)]
 token=w.HANDLE();sd=ctypes.c_void_p();owner=ctypes.c_void_p();dacl=ctypes.c_void_p()
 checked(api.OpenProcessToken(kernel.GetCurrentProcess(),8,ctypes.byref(token)))
 try:
  size=w.DWORD();api.GetTokenInformation(token,1,None,0,ctypes.byref(size));buffer=ctypes.create_string_buffer(size.value);checked(api.GetTokenInformation(token,1,buffer,size,ctypes.byref(size)));current_sid=sid_text(ctypes.cast(buffer,ctypes.POINTER(User)).contents.sid)
  status=api.GetNamedSecurityInfoW(str(path),1,5,ctypes.byref(owner),None,ctypes.byref(dacl),None,ctypes.byref(sd))
  if status:raise ctypes.WinError(status)
  if not owner or not dacl:raise ValueError('missing owner or null DACL is not private')
  control=w.WORD();revision=w.DWORD();checked(api.GetSecurityDescriptorControl(sd,ctypes.byref(control),ctypes.byref(revision)));acl=ctypes.cast(dacl,ctypes.POINTER(ACL)).contents;aces=[]
  if acl.count>64:raise ValueError('private anchor ACL is not bounded')
  for offset in range(acl.count):
   pointer=ctypes.c_void_p();checked(api.GetAce(dacl,offset,ctypes.byref(pointer)));ace=ctypes.cast(pointer,ctypes.POINTER(ACE)).contents
   if ace.type!=0 or ace.size<20:raise ValueError('private anchor contains unsupported ACL entry')
   aces.append({'type':int(ace.type),'flags':int(ace.flags),'mask':int(ace.mask),'sid':sid_text(pointer.value+ctypes.sizeof(ACE))})
  return validate_acl_observation({'owner_sid':sid_text(owner),'current_sid':current_sid,'protected':bool(control.value&0x1000),'aces':aces})
 finally:
  if sd:kernel.LocalFree(sd)
  kernel.CloseHandle(token)

def validate_training_output(anchor,output,frozen_roots):
 anchor,output=Path(anchor),Path(output)
 if not anchor.is_absolute() or not output.is_absolute() or output!=anchor/'result' or not anchor.is_dir() or anchor.resolve()!=anchor:raise ValueError('exact precreated canonical training anchor/result required')
 for part in (anchor,*anchor.parents):
  stat=part.lstat()
  if part.is_symlink() or getattr(stat,'st_file_attributes',0)&0x400:raise ValueError('training anchor ancestry is redirected')
 if output.exists() or output.is_symlink() or any(anchor.iterdir()):raise ValueError('fresh empty anchor and absent result required')
 for frozen in frozen_roots:
  frozen=Path(frozen).resolve()
  if anchor.is_relative_to(frozen) or frozen.is_relative_to(anchor):raise ValueError('training output overlaps frozen input/script roots')
 return private_acl(anchor)
