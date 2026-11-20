# SPDX-License-Identifier: Apache-2.0
# Inspect only the physical scopes the resumed Windows service may use.
param([switch]$After)
$ErrorActionPreference='Stop'
$workspace=(Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$campaign=Join-Path $workspace 'artifacts/v190/final-plan/exec-20261116T052942Z'
$prior=Join-Path $campaign 'windows-managed-resource-policy-r1/fixture'
$sid='S-1-5-21-3678370870-3858642417-826968105-1002'
. (Join-Path $workspace 'scripts/dev/assert-human-execution.ps1')
$human=Assert-AirkHumanExecution $sid
$roots=@('install','state','source','data')|ForEach-Object{Join-Path $prior $_}
if($After){$roots+=Join-Path $campaign 'windows-managed-resource-policy-r4'}
$counts=@{}
foreach($root in $roots){
    $null=Assert-AirkCreatedOutput $root $workspace $sid
    $folders=[Collections.Generic.Stack[IO.DirectoryInfo]]::new()
    $folders.Push((Get-Item -LiteralPath $root)); $dirs=0; $files=0
    while($folders.Count){
        $folder=$folders.Pop()
        if($folder.Attributes -band [IO.FileAttributes]::ReparsePoint){throw "Redirected directory $folder"}
        $acl=[IO.FileSystemAclExtensions]::GetAccessControl($folder)
        if($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid){throw "Foreign directory $folder"}
        $null=Assert-AirkDirectoryPolicy $acl $human
        $dirs++
        foreach($entry in $folder.EnumerateFileSystemInfos()){
            if($entry.Attributes -band [IO.FileAttributes]::ReparsePoint){throw "Redirected output $entry"}
            if($entry -is [IO.DirectoryInfo]){$folders.Push($entry);continue}
            $acl=[IO.FileSystemAclExtensions]::GetAccessControl($entry)
            if($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid){throw "Foreign output $entry"}
            $files++
        }
    }
    $counts[$root]=@{files=$files;directories=$dirs}
}
@{owner=$sid;roots=$counts;after=$After.IsPresent}|ConvertTo-Json -Depth 4 -Compress

