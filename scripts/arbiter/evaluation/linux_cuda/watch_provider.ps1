# SPDX-License-Identifier: Apache-2.0
# Sample the original Ubuntu and its Windows backing while the authorized native build runs.
param([Parameter(Mandatory)][string]$ReceiptRoot, [ValidateSet('provider','core','archive')][string]$Stage = 'provider')
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path .).Path
$allowed = Join-Path $repo 'artifacts/v190/final-plan/exec-20261116T052942Z'
if ((Resolve-Path $ReceiptRoot).Path -ne $allowed) { throw 'Unexpected receipt root' }
$sid = 'S-1-5-21-3678370870-3858642417-826968105-1002'
. ./scripts/dev/assert-human-execution.ps1
$null = Assert-AirkHumanExecution $sid
$observed = @{provider='linux-cuda-ort-r4-provider-r3.json';core='linux-cuda-ort-r4-core-r1.json';archive='linux-cuda-ort-final-core-r1.json'}
$baseline = Join-Path $allowed 'linux-cuda-r4-provider-r3-space-watch.jsonl'
$log = if ($Stage -eq 'provider') { $baseline } else { Join-Path $allowed "linux-cuda-r4-$Stage-r1-space-watch.jsonl" }
$started = [DateTimeOffset]::UtcNow
$null = Assert-AirkOutputParent $log $repo $sid
if (Test-Path $log) { throw 'Existing observation journal' }
$distro = @(Get-ChildItem HKCU:/Software/Microsoft/Windows/CurrentVersion/Lxss | Get-ItemProperty | Where-Object DistributionName -eq Ubuntu)
if ($distro.Count -ne 1 -or $distro[0].DefaultUid -ne 1000) { throw 'Unexpected Ubuntu identity' }
$vhd = (Resolve-Path (Join-Path $distro[0].BasePath $distro[0].VhdFileName)).Path
if ([IO.Path]::GetPathRoot($vhd) -ne 'C:\') { throw 'Unknown Ubuntu backing drive' }
$probe = 'set -eu;test "$(id -un):$(id -u):$(id -g):$(getent passwd 1000|cut -d: -f1)" = tyler:1000:1000:tyler;df -B1 --output=avail /home/tyler | tail -1'
$signal = 'set -eu;test "$(id -un):$(id -u):$(id -g):$(getent passwd 1000|cut -d: -f1)" = tyler:1000:1000:tyler;cd /mnt/d/code/Github/airk/scripts/arbiter/evaluation/linux_cuda;exec /usr/bin/python3 -B -S checkpoint_ort.py --phase request'
if ($Stage -ne 'provider') {
    $signal = $signal.Replace('checkpoint_ort.py --phase request', "stop_native_stage.py --stage $Stage")
}
$limit = 24GB
$first = if ($Stage -ne 'provider') { Get-Content -LiteralPath $baseline -TotalCount 1 | ConvertFrom-Json } else { $null }
do {
    $raw = wsl.exe -d Ubuntu -u tyler --exec /bin/sh -c $probe
    if ($LASTEXITCODE) { throw 'Guest identity or disk probe failed' }
    $m = Get-CimInstance Win32_PerfFormattedData_PerfOS_Memory
    $snap = [ordered]@{utc=[DateTimeOffset]::UtcNow.ToString('o');guest_free=[long]$raw.Trim();backing_free=(Get-PSDrive C).Free;vhd_length=(Get-Item -LiteralPath $vhd).Length;ram_available=[long]$m.AvailableBytes;commit_available=[long]$m.CommitLimit-[long]$m.CommittedBytes}
    if (!$first) { $first = [pscustomobject]@{guest_free=$snap.guest_free;vhd_length=$snap.vhd_length;backing_free=$snap.backing_free} }
    $vhdGrowth = [Math]::Max([long]0,[Math]::Max([long]($snap.vhd_length-$first.vhd_length),[long]($first.backing_free-$snap.backing_free)))
    $guestGrowth = [Math]::Max([long]0,[long]($first.guest_free-$snap.guest_free))
    $snap.stop = ($vhdGrowth -gt $limit -or $guestGrowth -gt $limit -or $snap.backing_free-($limit-$vhdGrowth) -lt 100GB -or $snap.guest_free-($limit-$guestGrowth) -lt 100GB -or $snap.ram_available -lt 8GB -or $snap.commit_available -lt 8GB)
    if ($snap.stop) {
        $snap.checkpoint = @(wsl.exe -d Ubuntu -u tyler --exec /bin/sh -c $signal)
        $snap.checkpoint_exit = $LASTEXITCODE
    }
    [IO.File]::AppendAllText($log,($snap | ConvertTo-Json -Compress)+[char]10)
    $null = Assert-AirkCreatedOutput $log $repo $sid
    if ($snap.stop) { throw 'Native build capacity limit reached' }
    if (Test-Path (Join-Path $allowed $observed[$Stage])) { break }
    if (($Stage -eq 'provider' -and [DateTimeOffset]::UtcNow.ToUnixTimeSeconds() -gt 1795159659) -or
        ([DateTimeOffset]::UtcNow-$started).TotalSeconds -gt 11000) { break }
    Start-Sleep -Seconds 30
} while ($true)
