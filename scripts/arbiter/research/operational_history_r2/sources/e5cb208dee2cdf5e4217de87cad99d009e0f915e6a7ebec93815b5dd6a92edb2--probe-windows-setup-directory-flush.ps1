# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference='Stop'
Add-Type -TypeDefinition @"
using System;
using System.IO;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public static class AirkDirectoryFlushProbe {
 [StructLayout(LayoutKind.Sequential)] public struct IOS { public IntPtr Status; public UIntPtr Information; }
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern SafeFileHandle CreateFileW(string name,uint access,uint share,IntPtr security,uint disposition,uint flags,IntPtr template);
 [DllImport("ntdll.dll")] static extern int NtFlushBuffersFileEx(SafeFileHandle file,uint flags,IntPtr parameters,uint size,out IOS ios);
 public static object Probe(string directory,uint access) {
  using(var file=CreateFileW(directory,access,7,IntPtr.Zero,3,0x02200000,IntPtr.Zero)) {
   if(file.IsInvalid)return new { access=access,opened=false,win32=Marshal.GetLastWin32Error(),status="not_opened" };
   IOS ios;int status=NtFlushBuffersFileEx(file,0,IntPtr.Zero,0,out ios);
   return new { access=access,opened=true,win32=0,status="0x"+((uint)status).ToString("x8") };
  }
 }
}
"@
$probeBase=Join-Path ([System.IO.Path]::GetTempPath()) ('airk-setup-flush-'+[Guid]::NewGuid().ToString('N'))
[System.IO.Directory]::CreateDirectory($probeBase) | Out-Null
try {
 $results=@(0xA0,0xA2,0xA4,0x1200A2,0x1200A4,3221225472) | ForEach-Object { [AirkDirectoryFlushProbe]::Probe($probeBase,[uint32]$_) }
 [pscustomobject]@{schema=1;kind='owned_windows_directory_flush_probe';os=[Environment]::OSVersion.VersionString;results=$results} | ConvertTo-Json -Depth 5
} finally { [System.IO.Directory]::Delete($probeBase,$false) }
