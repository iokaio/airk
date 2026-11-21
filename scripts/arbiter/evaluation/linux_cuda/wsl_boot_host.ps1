# SPDX-License-Identifier: Apache-2.0
# Read-only Windows observation for wsl_boot_admission.py. It writes nothing, starts no
# process and runs no wsl command. Output is one JSON object on stdout.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ExpectedSid,
    [string]$VolumeLog,
    [string]$ReceiptPath,
    [switch]$Created
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../../../..')).Path
. (Join-Path $root 'scripts/dev/assert-human-execution.ps1')
$identity = Assert-AirkHumanExecution $ExpectedSid
$campaign = (Resolve-Path -LiteralPath (Join-Path $root 'artifacts/v190/final-plan/exec-20261116T052942Z')).Path
$token = [Security.Principal.WindowsIdentity]::GetCurrent()
try {
    $elevated = [Security.Principal.WindowsPrincipal]::new($token).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
} finally { $token.Dispose() }

function Get-AirkCampaignFile {
    param([string]$Path, [bool]$MustExist)
    # The guard checks the parent chain and, for an existing file, the human owner.
    $checked = if ($MustExist) { Assert-AirkCreatedOutput $Path $campaign $ExpectedSid } else { Assert-AirkOutputParent $Path $campaign $ExpectedSid }
    if ([IO.Path]::GetDirectoryName($checked.Path) -ne $campaign) { throw 'Expected a file directly inside the campaign root' }
    [ordered]@{ path = $checked.Path; exists = (Test-Path -LiteralPath $checked.Path -PathType Leaf); owner_checked = $MustExist }
}
$volumeLogFact = if ($VolumeLog) { Get-AirkCampaignFile $VolumeLog $true } else { $null }
$receiptFact = if ($ReceiptPath) { Get-AirkCampaignFile $ReceiptPath ([bool]$Created) } else { $null }

$lxss = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Lxss'
$defaultKey = (Get-ItemProperty -LiteralPath $lxss).DefaultDistribution
$distributions = @(Get-ChildItem -LiteralPath $lxss | ForEach-Object {
    $entry = Get-ItemProperty -LiteralPath $_.PSPath
    $vhd = $null; $length = $null
    if ($entry.BasePath) {
        $base = $entry.BasePath -replace '^\\\\\?\\', ''
        $vhd = Join-Path $base $(if ($entry.VhdFileName) { $entry.VhdFileName } else { 'ext4.vhdx' })
        if (Test-Path -LiteralPath $vhd -PathType Leaf) { $length = (Get-Item -LiteralPath $vhd).Length }
    }
    [ordered]@{ name = [string]$entry.DistributionName; default = ($_.PSChildName -eq $defaultKey)
        default_uid = [long]$entry.DefaultUid; version = [int]$entry.Version; vhd_path = $vhd; vhd_length = $length }
})
$ubuntu = @($distributions | Where-Object { $_.name -ceq 'Ubuntu' })
if ($ubuntu.Count -ne 1 -or -not $ubuntu[0].vhd_path) { throw 'Expected the registered Ubuntu distribution' }
$windowsDrive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($campaign))
$ubuntuDrive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($ubuntu[0].vhd_path))
if ($windowsDrive.Name -ne 'D:\' -or $ubuntuDrive.Name -ne 'C:\') { throw 'Reconcile changed campaign volume mapping before continuing' }

# WSL keeps its swap file in the user's temporary folder, directly or inside one GUID-named folder,
# unless .wslconfig names another file. It exists only while the virtual machine runs.
$wslconfig = [ordered]@{ path = (Join-Path $env:USERPROFILE '.wslconfig'); present = $false }
$candidates = [Collections.Generic.List[string]]::new()
if (Test-Path -LiteralPath $wslconfig.path -PathType Leaf) {
    $wslconfig.present = $true
    foreach ($line in Get-Content -LiteralPath $wslconfig.path) {
        if ($line -match '^\s*(memory|processors|swap|swapFile|vmIdleTimeout)\s*=\s*(.*?)\s*$') { $wslconfig[$Matches[1].ToLowerInvariant()] = $Matches[2] }
    }
    if ($wslconfig['swapfile']) { $candidates.Add(($wslconfig['swapfile'] -replace '\\\\', '\')) }
}
$temp = Join-Path $env:LOCALAPPDATA 'Temp'
$candidates.Add((Join-Path $temp 'swap.vhdx'))
foreach ($folder in Get-ChildItem -LiteralPath $temp -Directory -ErrorAction SilentlyContinue) { $candidates.Add((Join-Path $folder.FullName 'swap.vhdx')) }
# Sort-Object -Unique, not Select-Object -Unique: the latter is quadratic and took about 90 s over the
# tens of thousands of folders this temporary folder can hold, which outlasted the guest's idle period.
$swapFiles = @($candidates | Sort-Object -Unique | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | ForEach-Object {
    $file = Get-Item -LiteralPath $_
    [ordered]@{ path = $file.FullName; length = $file.Length }
})

$taskName = 'WSL LLM Server (keep Ubuntu running)'
$tasks = @(Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue)
$taskState = if ($tasks.Count -eq 1) { [string]$tasks[0].State } elseif ($tasks.Count -eq 0) { 'Absent' } else { 'Ambiguous' }
$processNames = 'vmmemWSL', 'vmmem', 'wslhost', 'wsl', 'wslrelay', 'com.docker.backend', 'Docker Desktop', 'ollama'
$processes = @(Get-Process -Name $processNames -ErrorAction SilentlyContinue | Sort-Object ProcessName, Id | ForEach-Object {
    [ordered]@{ name = $_.ProcessName; pid = $_.Id }
})
$memory = Get-CimInstance Win32_PerfFormattedData_PerfOS_Memory

[ordered]@{
    schema = 'airk.private.wsl-boot-host/1'
    utc = [DateTimeOffset]::UtcNow.ToString('o')
    identity = [ordered]@{ name = $identity.UserName; sid = $identity.UserSid; owner_sid = $identity.OwnerSid
        session = $identity.SessionId; interactive_owner_sids = @($identity.InteractiveOwnerSids); elevated = $elevated }
    c_free = $ubuntuDrive.AvailableFreeSpace
    d_free = $windowsDrive.AvailableFreeSpace
    distributions = $distributions
    swap_files = $swapFiles
    wslconfig = $wslconfig
    keep_alive_task = [ordered]@{ name = $taskName; state = $taskState }
    processes = $processes
    memory = [ordered]@{ physical_available_bytes = [long]$memory.AvailableBytes; commit_limit_bytes = [long]$memory.CommitLimit
        commit_bytes = [long]$memory.CommittedBytes; commit_available_bytes = [long]$memory.CommitLimit - [long]$memory.CommittedBytes }
    volume_log = $volumeLogFact
    receipt = $receiptFact
} | ConvertTo-Json -Compress -Depth 6
