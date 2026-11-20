# SPDX-License-Identifier: Apache-2.0
# Observe host-volume growth during the Windows and WSL CUDA campaign.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ExpectedSid,
    [Parameter(Mandatory)][string]$OutputPath,
    [Parameter(Mandatory)][DateTimeOffset]$NotAfter,
    [Parameter(Mandatory)][long]$InitialWindowsFreeBytes,
    [Parameter(Mandatory)][long]$WindowsReservationBytes,
    [Parameter(Mandatory)][long]$UbuntuBackingReservationBytes,
    [Parameter(Mandatory)][long]$RequiredFreeBytes,
    [ValidatePattern('^[a-fA-F0-9]{64}$')][string]$HeldBackingLogSha256
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../../..')).Path
. (Join-Path $root 'scripts/dev/assert-human-execution.ps1')
$identity = Assert-AirkHumanExecution $ExpectedSid
$campaign = (Resolve-Path -LiteralPath (Join-Path $root 'artifacts/v190/final-plan/exec-20261116T052942Z')).Path
$output = [IO.Path]::GetFullPath($OutputPath)
if ([IO.Path]::GetDirectoryName($output) -ne $campaign -or [IO.Path]::GetFileName($output) -notmatch '^host-volume-watch-r[1-9][0-9]*\.jsonl$') { throw 'Expected a fresh campaign volume log' }
if ($NotAfter -le [DateTimeOffset]::UtcNow -or $WindowsReservationBytes -le 0 -or $UbuntuBackingReservationBytes -le 0 -or $RequiredFreeBytes -le 0) { throw 'Invalid duration or volume allowances' }
$ubuntu = @(Get-ChildItem 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Lxss' | Get-ItemProperty | Where-Object DistributionName -eq 'Ubuntu')
if ($ubuntu.Count -ne 1) { throw 'Expected the registered Ubuntu distribution' }
$disk = (Get-Item -LiteralPath (Join-Path $ubuntu[0].BasePath 'ext4.vhdx')).FullName
$windowsDrive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($campaign))
$ubuntuDrive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($disk))
if ($windowsDrive.Name -ne 'D:\' -or $ubuntuDrive.Name -ne 'C:\') { throw 'Reconcile changed campaign volume mapping before continuing' }
$ubuntuStart = $ubuntuDrive.AvailableFreeSpace
$vhdStart = (Get-Item -LiteralPath $disk).Length
$heldVhd = $null
if ($HeldBackingLogSha256) {
    $prior = Join-Path $campaign 'host-volume-watch-r1.jsonl'
    $null = Assert-AirkCreatedOutput $prior $campaign $ExpectedSid
    if ((Get-FileHash -LiteralPath $prior).Hash -ne $HeldBackingLogSha256) { throw 'Original alert log changed' }
    $first = Get-Content -LiteralPath $prior -TotalCount 1 | ConvertFrom-Json
    $last = Get-Content -LiteralPath $prior -Tail 1 | ConvertFrom-Json
    if ($last.within_budget -ne $false -or $last.has_headroom -ne $true -or
        $first.windows_start -ne $InitialWindowsFreeBytes -or $first.windows_budget -ne $WindowsReservationBytes -or
        $first.backing_budget -ne $UbuntuBackingReservationBytes -or $first.free_floor -ne $RequiredFreeBytes -or
        $first.backing_start - $last.backing_free -le $UbuntuBackingReservationBytes -or
        $InitialWindowsFreeBytes - $last.windows_free -gt $WindowsReservationBytes) { throw 'Expected original C-only budget alert' }
    $ubuntuStart = [long]$first.backing_start
    $vhdStart = [long]$first.vhd_start
    $heldVhd = [long]$last.vhd_bytes
}
$null = Assert-AirkOutputPathObservation $output $campaign $identity
$log = [IO.File]::Open($output, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
try {
    $null = Assert-AirkCreatedOutput $output $campaign $ExpectedSid
    $writer = [IO.StreamWriter]::new($log, [Text.UTF8Encoding]::new($false))
    try {
        do {
            $now = [DateTimeOffset]::UtcNow
            $ubuntuFree = $ubuntuDrive.AvailableFreeSpace
            $windowsFree = $windowsDrive.AvailableFreeSpace
            $vhdSize = (Get-Item -LiteralPath $disk).Length
            $ubuntuGrowth = [Math]::Max([long]0, [Math]::Max($ubuntuStart - $ubuntuFree, $vhdSize - $vhdStart))
            $windowsGrowth = [Math]::Max([long]0, $InitialWindowsFreeBytes - $windowsFree)
            $withinBudget = $ubuntuGrowth -le $UbuntuBackingReservationBytes -and $windowsGrowth -le $WindowsReservationBytes
            $hasHeadroom = $ubuntuFree -ge $RequiredFreeBytes + [Math]::Max([long]0, $UbuntuBackingReservationBytes - $ubuntuGrowth) -and
                           $windowsFree -ge $RequiredFreeBytes + [Math]::Max([long]0, $WindowsReservationBytes - $windowsGrowth)
            $entry = [ordered]@{utc=$now.ToString('o'); windows_free=$windowsFree; backing_free=$ubuntuFree; vhd_bytes=$vhdSize;
                windows_start=$InitialWindowsFreeBytes; backing_start=$ubuntuStart; vhd_start=$vhdStart; windows_budget=$WindowsReservationBytes;
                backing_budget=$UbuntuBackingReservationBytes; free_floor=$RequiredFreeBytes; within_budget=$withinBudget; has_headroom=$hasHeadroom}
            if ($null -ne $heldVhd) {
                $entry['backing_writes_held'] = $true
                $entry['original_alert_log_sha256'] = $HeldBackingLogSha256
            }
            $writer.WriteLine(($entry | ConvertTo-Json -Compress))
            $writer.Flush()
            if ($null -ne $heldVhd) {
                if ($vhdSize -gt $heldVhd -or $windowsGrowth -gt $WindowsReservationBytes -or !$hasHeadroom) {
                    throw 'New VHD growth, Windows budget alert or physical floor alert during the C: write hold'
                }
            } elseif (!$withinBudget -or !$hasHeadroom) {
                throw 'Volume reservation or free-space floor crossed; hold new writers and review sample'
            }
            $remaining = ($NotAfter - $now).TotalSeconds
            if ($remaining -le 0) { break }
            Start-Sleep -Seconds ([Math]::Min(30, $remaining))
        } while ($true)
    } finally { $writer.Dispose() }
} finally { $log.Dispose() }
