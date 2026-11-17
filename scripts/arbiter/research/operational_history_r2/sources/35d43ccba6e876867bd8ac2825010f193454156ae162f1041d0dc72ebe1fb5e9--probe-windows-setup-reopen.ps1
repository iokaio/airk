# SPDX-License-Identifier: Apache-2.0
# Bounded owned-fixture OS probe only; not an AIRK build or qualification run.
$ErrorActionPreference='Stop'
Add-Type -TypeDefinition @"
using System;
using System.IO;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public static class AirkSetupReopenProbe {
 [StructLayout(LayoutKind.Sequential)] struct UString { public ushort Length,MaximumLength; public IntPtr Buffer; }
 [StructLayout(LayoutKind.Sequential)] struct OAttrs { public uint Length; public IntPtr RootDirectory,ObjectName; public uint Attributes; public IntPtr SecurityDescriptor,SecurityQualityOfService; }
 [StructLayout(LayoutKind.Sequential)] struct IOS { public IntPtr Status; public UIntPtr Information; }
 [StructLayout(LayoutKind.Sequential)] struct FileInfo { public uint Attributes; public System.Runtime.InteropServices.ComTypes.FILETIME Creation,Access,Write; public uint Volume,SizeHigh,SizeLow,Links,IndexHigh,IndexLow; }
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern SafeFileHandle CreateFileW(string name,uint access,uint share,IntPtr security,uint disposition,uint flags,IntPtr template);
 [DllImport("kernel32.dll",SetLastError=true)] static extern SafeFileHandle ReOpenFile(SafeFileHandle original,uint access,uint share,uint flags);
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetFileInformationByHandle(SafeFileHandle file,out FileInfo information);
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetFileInformationByHandleEx(SafeFileHandle file,int kind,IntPtr output,uint size);
 [DllImport("ntdll.dll")] static extern int NtCreateFile(out IntPtr file,uint access,ref OAttrs attributes,out IOS ios,IntPtr allocation,uint fileAttributes,uint share,uint disposition,uint options,IntPtr ea,uint eaLength);
 [DllImport("ntdll.dll")] static extern int NtFlushBuffersFileEx(SafeFileHandle file,uint flags,IntPtr parameters,uint size,out IOS ios);
 [DllImport("ntdll.dll")] static extern uint RtlNtStatusToDosError(int status);
 static string Status(int value) { return "0x"+((uint)value).ToString("x8"); }
 static string Identity(SafeFileHandle file) { FileInfo info;if(!GetFileInformationByHandle(file,out info))return "error:"+Marshal.GetLastWin32Error();return info.Volume.ToString("x8")+":"+info.IndexHigh.ToString("x8")+info.IndexLow.ToString("x8"); }
 static SafeFileHandle Native(SafeFileHandle root,string relative,uint access,uint disposition,uint options,out int status) {
  IntPtr text=Marshal.StringToHGlobalUni(relative),name=IntPtr.Zero;
  try {
   UString value=new UString{Length=(ushort)(relative.Length*2),MaximumLength=(ushort)(relative.Length*2),Buffer=text};
   name=Marshal.AllocHGlobal(Marshal.SizeOf(typeof(UString)));Marshal.StructureToPtr(value,name,false);
   OAttrs attrs=new OAttrs{Length=(uint)Marshal.SizeOf(typeof(OAttrs)),RootDirectory=root==null?IntPtr.Zero:root.DangerousGetHandle(),ObjectName=name,Attributes=0x40};
   IOS ios;IntPtr raw;status=NtCreateFile(out raw,access,ref attrs,out ios,IntPtr.Zero,0x80,7,disposition,options,IntPtr.Zero,0);
   return new SafeFileHandle(status<0?new IntPtr(-1):raw,true);
  } finally {if(name!=IntPtr.Zero)Marshal.FreeHGlobal(name);Marshal.FreeHGlobal(text);}
 }
 static object Inspect(string mode,uint access,uint flags,SafeFileHandle reopened,int nativeStatus,string expected) {
  int error=reopened.IsInvalid?Marshal.GetLastWin32Error():0;
  if(reopened.IsInvalid&&mode=="nt_empty")error=(int)RtlNtStatusToDosError(nativeStatus);
  string identity=reopened.IsInvalid?null:Identity(reopened),flush="not_opened";bool inventory=false;int inventoryError=0;
  if(!reopened.IsInvalid) {
   IOS ios;flush=Status(NtFlushBuffersFileEx(reopened,0,IntPtr.Zero,0,out ios));
   IntPtr buffer=Marshal.AllocHGlobal(65536);
   try {inventory=GetFileInformationByHandleEx(reopened,11,buffer,65536);if(!inventory)inventoryError=Marshal.GetLastWin32Error();}finally{Marshal.FreeHGlobal(buffer);}
  }
  return new { mode=mode,requested_access=access,flags=flags,opened=!reopened.IsInvalid,win32_error=error,nt_status=mode=="nt_empty"?Status(nativeStatus):null,identity=identity,identity_matches=identity!=null&&identity==expected,flush_status=flush,inventory=inventory,inventory_error=inventoryError };
 }
 public static object Run(string directory) {
  var rows=new List<object>();int serial=0;
  uint[] originalAccess={0xA0};
  uint[] requested={0xA0,0xA2,0x81,0x1000A2,0x100081};
  uint[] reopenFlags={0x02000000u,0x02200000u};
  using(var parent=CreateFileW(directory,0xA0,7,IntPtr.Zero,3,0x02200000,IntPtr.Zero)) {
  if(parent.IsInvalid)throw new IOException("Owned original-parent open failed: "+Marshal.GetLastWin32Error());
  foreach(string origin in new[]{"create_file","nt_open","nt_create"})foreach(uint access in originalAccess) {
   string leaf="original-"+(serial++).ToString();string absolute=Path.Combine(directory,leaf);
   if(origin!="nt_create")Directory.CreateDirectory(absolute);
   int initialStatus=0;SafeFileHandle original;
   if(origin=="create_file")original=CreateFileW(absolute,access,7,IntPtr.Zero,3,0x02200000,IntPtr.Zero);
   else original=Native(parent,leaf,access|0x100000,origin=="nt_create"?2u:1u,0x200021,out initialStatus);
   using(original) {
    if(original.IsInvalid){rows.Add(new{origin=origin,original_access=access,opened=false,nt_status=Status(initialStatus),win32_error=Marshal.GetLastWin32Error()});continue;}
    File.WriteAllText(Path.Combine(absolute,"fixture.txt"),"owned fixture");
    string expected=Identity(original);var attempts=new List<object>();
    foreach(uint requestedAccess in requested) {
     foreach(uint flags in reopenFlags)using(var reopened=ReOpenFile(original,requestedAccess,7,flags))attempts.Add(Inspect("reopen_file",requestedAccess,flags,reopened,0,expected));
     foreach(uint options in new[]{0x200021u,0x21u}) {
      int status;using(var reopened=Native(original,"",requestedAccess|0x100000,1,options,out status))attempts.Add(Inspect("nt_empty",requestedAccess|0x100000,options,reopened,status,expected));
     }
    }
    rows.Add(new{origin=origin,original_access=access,opened=true,identity=expected,attempts=attempts});
   }
  }
  }
  return new{schema=1,kind="owned_native_windows_reopen_probe",original_cases=rows.Count,results=rows};
 }
}
"@
$reopenTempRoot=[System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$reopenProbeRoot=Join-Path $reopenTempRoot ('airk-setup-reopen-'+[Guid]::NewGuid().ToString('N'))
[System.IO.Directory]::CreateDirectory($reopenProbeRoot) | Out-Null
try { [AirkSetupReopenProbe]::Run($reopenProbeRoot) | ConvertTo-Json -Depth 12 }
finally {
 $checkedReopenRoot=[System.IO.Path]::GetFullPath($reopenProbeRoot)
 if(-not $checkedReopenRoot.StartsWith($reopenTempRoot,[System.StringComparison]::OrdinalIgnoreCase) -or -not ([System.IO.Path]::GetFileName($checkedReopenRoot)).StartsWith('airk-setup-reopen-')) { throw 'Owned fixture cleanup boundary refused' }
 [System.IO.Directory]::Delete($checkedReopenRoot,$true)
}
