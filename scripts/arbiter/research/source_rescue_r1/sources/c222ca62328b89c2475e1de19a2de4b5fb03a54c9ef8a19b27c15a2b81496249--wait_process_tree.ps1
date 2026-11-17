# SPDX-License-Identifier: Apache-2.0
param([Parameter(Mandatory=$true)][string]$Request)
$ErrorActionPreference = 'Stop'
$PSModuleAutoLoadingPreference = 'None'
Import-Module -Name 'C:/Program Files/PowerShell/7/Microsoft.PowerShell.Commands.Management.dll' -ErrorAction Stop
Import-Module -Name 'C:/Program Files/PowerShell/7/Microsoft.PowerShell.Commands.Utility.dll' -ErrorAction Stop
$req = Get-Content -LiteralPath $Request -Raw | ConvertFrom-Json
if ($req.schema -ne 'airk.private.process-tree-command/1') { throw 'wrong process-tree request' }
foreach ($path in @($req.stdout, $req.stderr, $req.result, $req.started)) { if (Test-Path -LiteralPath $path) { throw 'existing process-tree output preserved' } }
function Write-NewJson([string]$Path, $Value) {
    $bytes = [System.Text.Encoding]::UTF8.GetBytes(($Value | ConvertTo-Json -Depth 12 -Compress))
    $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write, [System.IO.FileShare]::Read)
    try { $stream.Write($bytes, 0, $bytes.Length); $stream.Flush($true) } finally { $stream.Dispose() }
}
function Signal-OwnedEvent([string]$Name) {
    if ($Name) { $event = [System.Threading.EventWaitHandle]::OpenExisting($Name); try { [void]$event.Set() } finally { $event.Dispose() } }
}
$script:leader = $null
$began = [DateTimeOffset]::UtcNow.ToString('O')
# Redirects force CreateProcess/UseShellExecute=false. Its nested Job is assigned
# before resume; -Wait keeps this leader alive until that tree completes.
Microsoft.PowerShell.Management\Start-Process -FilePath $req.executable -ArgumentList $req.arguments -WorkingDirectory $req.cwd -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput $req.stdout -RedirectStandardError $req.stderr -ErrorAction Stop | ForEach-Object {
    $script:leader = $_
    Write-NewJson $req.started @{ pid=$_.Id; wrapper_pid=$PID; started_utc=$began }
    Signal-OwnedEvent $req.started_event
}
if ($null -eq $script:leader) { throw 'missing actual command process' }
$script:leader.Refresh()
if (-not $script:leader.HasExited) { throw 'command process still active after tree wait' }
$code = $script:leader.ExitCode
if ($null -eq $code) { throw 'missing original command exit code' }
Write-NewJson $req.result @{ schema='airk.private.process-tree-result/1'; command_pid=$script:leader.Id; wrapper_pid=$PID; exit_code=$code; start_utc=$began; finished_utc=[DateTimeOffset]::UtcNow.ToString('O'); wait='Start-Process-Wait-process-and-descendants'; powershell=$PSVersionTable.PSVersion.ToString() }
Signal-OwnedEvent $req.done_event
$script:leader.Dispose()
exit $code
