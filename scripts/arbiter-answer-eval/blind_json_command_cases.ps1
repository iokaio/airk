# SPDX-License-Identifier: Apache-2.0
# Run a blind regression matrix against a disposable, checked offline Python CLI fixture.
[CmdletBinding()]
param([Parameter(Mandatory)][string]$Work, [Parameter(Mandatory)][string]$Plan,
      [Parameter(Mandatory)][string]$Python, [Parameter(Mandatory)][string]$Output)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
. (Join-Path $root 'scripts/dev/assert-human-execution.ps1')
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$null = Assert-AirkHumanExecution $sid
$boundary = Join-Path $root 'artifacts'
$folder = Get-AirkCheckedPath ([IO.Path]::GetFullPath($Work)) $boundary
$planPath = Get-AirkCheckedPath ([IO.Path]::GetFullPath($Plan)) $boundary
$out = Get-AirkCheckedPath ([IO.Path]::GetFullPath($Output)) $boundary
function Owned($path) { $null = Assert-AirkCreatedOutput $path $root $sid }
function CheckOutput($path) { $null = Assert-AirkOutputParent $path $root $sid }
Owned $folder
Owned $planPath
CheckOutput $out
if ((Test-Path $out) -or !(Test-Path -LiteralPath (Join-Path $folder '.blind-cli-disposable'))) { throw 'Expected fresh output and disposable fixture marker' }
Owned (Join-Path $folder '.blind-cli-disposable')
$spec = Get-Content -Raw -LiteralPath $planPath | ConvertFrom-Json -AsHashtable
$script = Get-AirkCheckedPath ([IO.Path]::GetFullPath($spec.entrypoint, $folder)) $folder
$data = Get-AirkCheckedPath ([IO.Path]::GetFullPath($spec.record, $folder)) $folder
Owned $script
Owned $data
CheckOutput $data
$exe = (Get-Item -LiteralPath $Python).FullName
$base = [IO.File]::ReadAllBytes($data)
if ((Get-FileHash $data).Hash -ne $spec.record_sha256) { throw 'Unexpected baseline record' }
$checks = @()
Push-Location $folder
try {
    foreach ($case in $spec.cases) {
        CheckOutput $data
        [IO.File]::WriteAllText($data, (ConvertTo-Json -InputObject $case.record -Depth 32))
        Owned $data
        $lines = @(& $exe -I -B $script 2>&1 | ForEach-Object { $_.ToString() })
        $rc = $LASTEXITCODE
        $content = $lines -join [Environment]::NewLine
        $checks += [ordered]@{ name=$case.name; expected_exit=$case.expected_exit; exit=$rc;
            traceback=($content -match 'Traceback \(most recent call last\)'); output=$content }
    }
} finally {
    Pop-Location
    CheckOutput $data
    [IO.File]::WriteAllBytes($data, $base)
    Owned $data
    if ((Get-FileHash $data).Hash -ne $spec.record_sha256) { throw 'Baseline not restored' }
}
$receipt = [ordered]@{ plan_sha256=(Get-FileHash $planPath).Hash; entry_sha256=(Get-FileHash $script).Hash;
    runtime=$exe; runtime_sha256=(Get-FileHash $exe).Hash; checks=$checks }
CheckOutput $out
[IO.File]::WriteAllText($out, ($receipt | ConvertTo-Json -Depth 8))
Owned $out
