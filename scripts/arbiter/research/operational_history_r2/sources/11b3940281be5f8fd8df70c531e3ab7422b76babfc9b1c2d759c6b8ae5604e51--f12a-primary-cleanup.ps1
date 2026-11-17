# SPDX-License-Identifier: Apache-2.0
param([ValidateSet('directories','processes')][string]$Observation)
$ErrorActionPreference='Stop'
$location=Get-Content artifacts/optimizations/f12a-runtime-location.json -Raw | ConvertFrom-Json
$taskPath=[IO.Path]::GetFullPath($location.task)
if ($taskPath -ne 'C:\Users\tdotn\AppData\Local\Temp\airk-optimizations-f12a-klfg0qdl') { throw 'Unexpected task root' }
$record=@{verified_utc=[DateTime]::UtcNow.ToString('o');task=$taskPath;observation=$Observation;deletions_performed=$false}
if ($Observation -eq 'directories') {
$task=Get-Item -LiteralPath $taskPath
if ($task.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Unexpected task reparse point' }
$directories=@()
foreach ($name in @('fixtures','target')) {
    $directory=Get-Item -LiteralPath (Join-Path $task.FullName $name)
    if ($directory.Parent.FullName -ne $task.FullName -or ($directory.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Unexpected fixture directory' }
    $entries=@(Get-ChildItem -LiteralPath $directory.FullName -Force)
    $directories+=@{name=$name;entries=$entries.Count}
    if ($entries.Count -ne 0) { throw "Fixture entries remain in $name" }
}
$record['directories']=$directories
} elseif ($Observation -eq 'processes') {
$prefix=$taskPath+'\'
$owned=@(Get-CimInstance Win32_Process | Where-Object {
    ($_.ExecutablePath -and $_.ExecutablePath.StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase)) -or
    ($_.CommandLine -and $_.CommandLine.IndexOf($prefix,[StringComparison]::OrdinalIgnoreCase) -ge 0)
})
if ($owned.Count -ne 0) { throw "Owned task processes remain: $($owned.Count)" }
$record['owned_task_processes']=$owned.Count
} else { throw 'Observation required' }
$destination=Join-Path (Get-Location) "artifacts/optimizations/f12a-primary-cleanup-$Observation.json"
if (Test-Path -LiteralPath $destination) { throw 'Receipt already exists' }
$record | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $destination
$record | ConvertTo-Json -Depth 4
