# SPDX-License-Identifier: Apache-2.0
# Same-human fixture admission and Edit/Write ownership checks for a Windows cost session.
param([ValidateSet('Prepare','Audit','BeforeTool','AfterTool','StageSkill')][string]$Mode,
      [Parameter(Mandatory)][string]$Session, [string]$Snapshot)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
. (Join-Path $root 'scripts/dev/assert-human-execution.ps1')
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$identity = Assert-AirkHumanExecution $sid
$sessionPath = [IO.Path]::GetFullPath($Session)
$work = Join-Path $sessionPath 'work'
function Parent($p) { $null = Assert-AirkOutputPathObservation $p $root $identity }
function Owned($p) {
    $item = Get-Item -Force -LiteralPath $p
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Unexpected link $p" }
    if ((Get-Acl -LiteralPath $p).GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid) { throw "Foreign output $p" }
}
function NewDirectory($p) { Parent $p; $null = New-Item -ItemType Directory -Path $p; Owned $p }
if ($Mode -eq 'StageSkill') {
    $dir = $sessionPath
    foreach ($part in 'airkctl','skills','airk-tools') {
        $dir = Join-Path $dir $part
        NewDirectory $dir
    }
    $file = Join-Path $dir 'SKILL.md'
    Parent $file
    [IO.File]::Copy($Snapshot, $file)
    Owned $file
    exit 0
}
if ($Mode -in @('BeforeTool','AfterTool')) {
    try {
        $hook = [Console]::In.ReadToEnd() | ConvertFrom-Json
        $p = [IO.Path]::GetFullPath([string]$hook.tool_input.file_path, $work)
        $null = Get-AirkCheckedPath $p $work
        if ($Mode -eq 'BeforeTool') {
            $missing = [Collections.Generic.Stack[string]]::new()
            $parentDir = [IO.Path]::GetDirectoryName($p)
            while (-not (Test-Path -LiteralPath $parentDir)) {
                $missing.Push($parentDir)
                $parentDir = [IO.Path]::GetDirectoryName($parentDir)
            }
            while ($missing.Count) { NewDirectory ($missing.Pop()) }
        }
        $null = Assert-AirkOutputPathObservation $p $work $identity
        if ($Mode -eq 'AfterTool') { Owned $p }
    } catch { [Console]::Error.WriteLine($_.Exception.Message); exit 2 }
    exit 0
}
if ($Mode -eq 'Audit') {
    Parent $work
    Owned $work
    foreach ($item in Get-ChildItem -LiteralPath $sessionPath -Recurse -Force) { Owned $item.FullName }
    exit 0
}
NewDirectory $sessionPath
NewDirectory $work
NewDirectory (Join-Path $sessionPath 'gather')
NewDirectory (Join-Path $sessionPath 'tmp')
$source = [IO.Path]::GetFullPath($Snapshot)
function CopyChecked($from,$to) {
    Assert-AirkCurrentToken $identity
    Assert-AirkDirectoryPolicy (Get-Acl -LiteralPath $to) $identity
    foreach ($item in Get-ChildItem -LiteralPath $from -Force) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Snapshot link $($item.Name)" }
        $dest = [IO.Path]::GetFullPath((Join-Path $to $item.Name))
        if ([IO.Path]::GetDirectoryName($dest) -ne $to -or (Test-Path -LiteralPath $dest)) {throw "Unexpected fixture output $dest"}
        if ($item.PSIsContainer) { NewDirectory $dest; CopyChecked $item.FullName $dest }
        else { [IO.File]::Copy($item.FullName,$dest); Owned $dest }
    }
}
CopyChecked $source $work
foreach ($name in 'tool.json','native-surface.json','unified-guidance.txt','mcp-config.json','stream.jsonl','stderr.log','answer.md','changes.patch','session.json') {
    Parent (Join-Path $sessionPath $name)
}
$logs = Join-Path $sessionPath 'gather'
Parent (Join-Path $logs 'calls.jsonl')
foreach ($i in 1..64) { Parent (Join-Path $logs ('call-{0:d3}.reply.json' -f $i)) }
