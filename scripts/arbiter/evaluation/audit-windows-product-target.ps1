# SPDX-License-Identifier: Apache-2.0
# Audit the existing sole-writer warm native target or newly staged product after a real build.
param([ValidateSet('target','product','cargo','shared-control','recovery')][string]$Kind, [string]$EntryLog)
$ErrorActionPreference='Stop'
$workspace=(Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$sid='S-1-5-21-3678370870-3858642417-826968105-1002'
. (Join-Path $workspace 'scripts/dev/assert-human-execution.ps1')
$human=Assert-AirkHumanExecution $sid
$root=if($Kind -eq 'target'){Join-Path $workspace 'target'}elseif($Kind -eq 'cargo'){Join-Path $workspace 'artifacts/v190/final-plan/exec-20261116T052942Z/windows-offline-cargo-r1'}elseif($Kind -eq 'shared-control'){Join-Path $workspace 'artifacts/v190/final-plan/exec-20261116T052942Z/cal100-focus1024-capture-r3/fixture/control'}elseif($Kind -eq 'recovery'){Join-Path $workspace 'artifacts/v190/final-plan/exec-20261116T052942Z/train-q598-closed-control-recovery-r2'}else{Join-Path $workspace 'artifacts/v190/final-plan/exec-20261116T052942Z/windows-fresh-cpu-product-r2'}
$null=Assert-AirkCreatedOutput $root $workspace $sid
$pending=[Collections.Generic.Stack[IO.DirectoryInfo]]::new()
$writer=$null
if($EntryLog){
    $campaign=(Resolve-Path (Join-Path $workspace 'artifacts/v190/final-plan/exec-20261116T052942Z')).Path
    $EntryLog=[IO.Path]::GetFullPath($EntryLog)
    if(!$EntryLog.StartsWith($campaign+'\',[StringComparison]::OrdinalIgnoreCase) -or $EntryLog.StartsWith($root+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Audit manifest must be under campaign, outside observed tree'}
    $null=Assert-AirkOutputParent $EntryLog $campaign $sid
    $writer=[IO.StreamWriter]::new([IO.File]::Open($EntryLog,[IO.FileMode]::CreateNew))
}
function Record-Entry($entry,[string]$kind){
    if($writer){$writer.WriteLine((@{path=[IO.Path]::GetRelativePath($root,$entry.FullName);kind=$kind;owner_sid=$sid;reparse=$false;inherited_directory_policy=($kind -eq 'directory')}|ConvertTo-Json -Compress))}
}
try{
$files=0;$dirs=0;$allocated=[long]0
if($Kind -eq 'target'){
    foreach($name in @('debug','cargo-timings')){
        $dir=Join-Path $root $name
        $null=Assert-AirkOutputParent $dir $workspace $sid
        if(Test-Path -LiteralPath $dir){$pending.Push((Get-Item -LiteralPath $dir))}
    }
    foreach($file in Get-ChildItem -LiteralPath $root -Force -File){$null=Assert-AirkCreatedOutput $file.FullName $workspace $sid;$files++}
}else{$pending.Push((Get-Item -LiteralPath $root))}
while($pending.Count){
    $folder=$pending.Pop()
    if($folder.Attributes -band [IO.FileAttributes]::ReparsePoint){throw "Reparse directory $folder"}
    $acl=[IO.FileSystemAclExtensions]::GetAccessControl($folder)
    if($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid){throw "Foreign directory $folder"}
    $null=Assert-AirkDirectoryPolicy $acl $human
    Record-Entry $folder 'directory'
    $dirs++;$allocated+=4096
    foreach($f in $folder.EnumerateFileSystemInfos()){
        if($f.Attributes -band [IO.FileAttributes]::ReparsePoint){throw "Reparse output $f"}
        if($f -is [IO.DirectoryInfo]){$pending.Push($f);continue}
        $acl=[IO.FileSystemAclExtensions]::GetAccessControl($f)
        if($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid){throw "Foreign output $f"}
        Record-Entry $f 'file'
        $files++;$allocated+=4096*[long][Math]::Ceiling($f.Length/4096)
    }
}
if($Kind -eq 'cargo' -and $allocated -gt 1.5GB){throw 'Shared Cargo cache exceeds original 1.5 GiB'}
}finally{if($writer){$writer.Dispose();$null=Assert-AirkCreatedOutput $EntryLog $campaign $sid}}
@{root=$root;owner=$sid;dirs=$dirs;files=$files;allocated_upper_bytes=$allocated;entry_log_sha256=$(if($EntryLog){(Get-FileHash $EntryLog).Hash})}|ConvertTo-Json -Compress
