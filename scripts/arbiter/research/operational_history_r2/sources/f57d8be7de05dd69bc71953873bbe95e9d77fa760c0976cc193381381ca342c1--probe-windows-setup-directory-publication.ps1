# SPDX-License-Identifier: Apache-2.0
# Owned-fixture native rename matrix; no AIRK build or global process state.
$ErrorActionPreference='Stop'
Add-Type -TypeDefinition @"
using System;using System.IO;using System.Text;using System.Collections.Generic;using System.Runtime.InteropServices;using Microsoft.Win32.SafeHandles;
public static class AirkDirectoryPublicationProbe {
 [StructLayout(LayoutKind.Sequential)]struct UString{public ushort Length,MaximumLength;public IntPtr Buffer;}
 [StructLayout(LayoutKind.Sequential)]struct OAttrs{public uint Length;public IntPtr RootDirectory,ObjectName;public uint Attributes;public IntPtr SecurityDescriptor,SecurityQualityOfService;}
 [StructLayout(LayoutKind.Sequential)]struct IOS{public IntPtr Status;public UIntPtr Information;}
 [StructLayout(LayoutKind.Sequential)]struct Rename{public uint Flags;public IntPtr Root;public uint Length;public ushort Name;}
 [StructLayout(LayoutKind.Sequential)]struct Overlap{public IntPtr Internal,InternalHigh;public uint Offset,OffsetHigh;public IntPtr Event;}
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)]static extern SafeFileHandle CreateFileW(string name,uint access,uint share,IntPtr security,uint disposition,uint flags,IntPtr template);
 [DllImport("kernel32.dll",SetLastError=true)]static extern bool LockFileEx(SafeFileHandle file,uint flags,uint reserved,uint low,uint high,ref Overlap overlap);
 [DllImport("ntdll.dll")]static extern int NtCreateFile(out IntPtr file,uint access,ref OAttrs attributes,out IOS ios,IntPtr allocation,uint fileAttributes,uint share,uint disposition,uint options,IntPtr ea,uint eaLength);
 [DllImport("ntdll.dll")]static extern int NtSetInformationFile(SafeFileHandle file,out IOS ios,IntPtr information,uint size,int kind);
 [DllImport("ntdll.dll")]static extern uint RtlNtStatusToDosError(int status);
 static SafeFileHandle Open(string path,uint access,bool directory){var handle=CreateFileW(path,access,7,IntPtr.Zero,3,directory?0x02200000u:0x00200000u,IntPtr.Zero);if(handle.IsInvalid)throw new IOException("Owned open failed "+Marshal.GetLastWin32Error());return handle;}
 static SafeFileHandle Relative(SafeFileHandle parent,string name,uint access,bool directory){
  name=name.Replace('/','\\');IntPtr text=Marshal.StringToHGlobalUni(name),unicode=IntPtr.Zero;
  try{UString value=new UString{Length=(ushort)(name.Length*2),MaximumLength=(ushort)(name.Length*2),Buffer=text};unicode=Marshal.AllocHGlobal(Marshal.SizeOf(typeof(UString)));Marshal.StructureToPtr(value,unicode,false);
   OAttrs attributes=new OAttrs{Length=(uint)Marshal.SizeOf(typeof(OAttrs)),RootDirectory=parent.DangerousGetHandle(),ObjectName=unicode,Attributes=0x40};
   IOS ios;IntPtr raw;int status=NtCreateFile(out raw,access|0x100000,ref attributes,out ios,IntPtr.Zero,0x80,7,1,(directory?1u:0x40u)|0x200020u,IntPtr.Zero,0);
   if(status<0)throw new IOException("Owned relative open failed 0x"+((uint)status).ToString("x8"));return new SafeFileHandle(raw,true);
  }finally{if(unicode!=IntPtr.Zero)Marshal.FreeHGlobal(unicode);Marshal.FreeHGlobal(text);}
 }
 static int Publish(SafeFileHandle source,SafeFileHandle destination,string name){
  byte[] bytes=Encoding.Unicode.GetBytes(name);int offset=Marshal.OffsetOf(typeof(Rename),"Name").ToInt32(),length=Marshal.SizeOf(typeof(Rename))+bytes.Length;
  IntPtr buffer=Marshal.AllocHGlobal(length);try{for(int i=0;i<length;i++)Marshal.WriteByte(buffer,i,0);Marshal.StructureToPtr(new Rename{Root=destination.DangerousGetHandle(),Length=(uint)bytes.Length},buffer,false);Marshal.Copy(bytes,0,IntPtr.Add(buffer,offset),bytes.Length);IOS ios;return NtSetInformationFile(source,out ios,buffer,(uint)length,10);}finally{Marshal.FreeHGlobal(buffer);}
 }
 public static object Run(string root){var rows=new List<object>();int count=0;
  foreach(string origin in new[]{"win32_absolute","nt_relative"})foreach(string role in new[]{"none","child_dir","nested_dir","child_file","nested_file","child_file_locked","all_dirs_and_locked_files","close_dirs_keep_locked_files"}) {
   string owner=Path.Combine(root,"case-"+(count++)),stage=Path.Combine(owner,"stage");Directory.CreateDirectory(stage);Directory.CreateDirectory(Path.Combine(stage,"namespace/collection/slot"));
   File.WriteAllText(Path.Combine(stage,"owner.lock"),"owned");File.WriteAllText(Path.Combine(stage,"namespace/collection/writer.lock"),"owned");
   var held=new List<SafeFileHandle>();
   using(var parent=Open(owner,0xA0,true))using(var rootPin=origin=="nt_relative"?Relative(parent,"stage",0xA0,true):Open(stage,0xA0,true))using(var source=origin=="nt_relative"?Relative(parent,"stage",0x110080,true):Open(stage,0x110080,true))try{
    if(role=="child_dir"||role=="all_dirs_and_locked_files")held.Add(origin=="nt_relative"?Relative(rootPin,"namespace",0xA0,true):Open(Path.Combine(stage,"namespace"),0xA0,true));
    if(role=="nested_dir"||role=="all_dirs_and_locked_files")held.Add(origin=="nt_relative"?Relative(rootPin,"namespace/collection/slot",0xA0,true):Open(Path.Combine(stage,"namespace/collection/slot"),0xA0,true));
    if(role=="child_file")held.Add(origin=="nt_relative"?Relative(rootPin,"owner.lock",0x100083,false):Open(Path.Combine(stage,"owner.lock"),0x100083,false));
    if(role=="nested_file")held.Add(origin=="nt_relative"?Relative(rootPin,"namespace/collection/writer.lock",0x100083,false):Open(Path.Combine(stage,"namespace/collection/writer.lock"),0x100083,false));
    if(role=="close_dirs_keep_locked_files")using(var closed=origin=="nt_relative"?Relative(rootPin,"namespace/collection/slot",0xA0,true):Open(Path.Combine(stage,"namespace/collection/slot"),0xA0,true)){}
    if(role=="child_file_locked"||role=="all_dirs_and_locked_files"||role=="close_dirs_keep_locked_files"){
     foreach(string name in new[]{"owner.lock","namespace/collection/writer.lock"}){var file=origin=="nt_relative"?Relative(rootPin,name,0x100083,false):Open(Path.Combine(stage,name),0x100083,false);Overlap overlap=new Overlap();if(!LockFileEx(file,3,0,uint.MaxValue,uint.MaxValue,ref overlap)){file.Dispose();throw new IOException("Owned lock failed "+Marshal.GetLastWin32Error());}held.Add(file);}
    }
    int status=Publish(source,parent,"published");rows.Add(new{origin=origin,role=role,held_handles=held.Count,nt_status="0x"+((uint)status).ToString("x8"),win32_error=status<0?RtlNtStatusToDosError(status):0,published=Directory.Exists(Path.Combine(owner,"published")),source_preserved=Directory.Exists(stage)});
   }finally{foreach(var file in held)file.Dispose();}
  }
  return new{schema=1,kind="owned_windows_directory_publication_probe",cases=rows};
 }
}
"@
$publicationTempRoot=[System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$publicationProbeRoot=Join-Path $publicationTempRoot ('airk-setup-publication-'+[Guid]::NewGuid().ToString('N'))
[System.IO.Directory]::CreateDirectory($publicationProbeRoot) | Out-Null
try { [AirkDirectoryPublicationProbe]::Run($publicationProbeRoot) | ConvertTo-Json -Depth 8 }
finally {
 $checkedPublicationRoot=[System.IO.Path]::GetFullPath($publicationProbeRoot)
 if(-not $checkedPublicationRoot.StartsWith($publicationTempRoot,[System.StringComparison]::OrdinalIgnoreCase) -or -not ([System.IO.Path]::GetFileName($checkedPublicationRoot)).StartsWith('airk-setup-publication-')){throw 'Owned fixture cleanup boundary refused'}
 [System.IO.Directory]::Delete($checkedPublicationRoot,$true)
}
