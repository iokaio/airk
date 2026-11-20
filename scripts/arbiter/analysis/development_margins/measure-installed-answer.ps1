# SPDX-License-Identifier: Apache-2.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Arm,
    [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{64}$')][string]$ArmSha256,
    [Parameter(Mandatory)][ValidatePattern('^windows-installed-(compact|large)-(cpu|cuda)-answer-r[1-9][0-9]*$')][string]$Run
)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../..'))
$campaign = Join-Path $repo 'artifacts/v190/final-plan/exec-20261116T052942Z'
$target = Join-Path $campaign $Run
$sid = 'S-1-5-21-3678370870-3858642417-826968105-1002'
. (Join-Path $repo 'scripts/dev/assert-human-execution.ps1')
$null = Assert-AirkHumanExecution $sid
$null = Assert-AirkOutputParent $target $campaign $sid
if (Test-Path -LiteralPath $target) { throw 'A fresh timing output is required' }
$armPath = (Resolve-Path -LiteralPath $Arm).Path
if ((Get-FileHash -LiteralPath $armPath).Hash.ToLowerInvariant() -ne $ArmSha256) { throw 'Changed timing arm specification' }
$spec = Get-Content -LiteralPath $armPath -Raw | ConvertFrom-Json -AsHashtable
$model = $spec.model -replace '^context_', ''
if ($Run -notmatch "^windows-installed-$model-$($spec.provider)-answer-r[1-9][0-9]*$") { throw 'Output and arm identities differ' }
$null = New-Item -ItemType Directory -Path $target
$null = Assert-AirkCreatedOutput $target $campaign $sid
$code = 1
try {
    $python = 'C:/Users/tdotn/AppData/Local/Programs/Python/Python313/python.exe'
    & $python -I -B -S (Join-Path $PSScriptRoot 'installed_answer_timing.py') --arm $armPath --arm-sha256 $ArmSha256 --output $target
    $code = $LASTEXITCODE
} finally {
    $files = @(Get-ChildItem -LiteralPath $target -Recurse -Force)
    for ($i = 0; $i -lt $files.Count; $i += 28) {
        $last = [Math]::Min($i + 27, $files.Count - 1)
        $entries = @(foreach ($file in $files[$i..$last]) {
            [pscustomobject]@{ path = $file.FullName; root = $target; created = $true }
        })
        $checked = Assert-AirkOutputBatch $entries $sid 10
        if ($checked.expired -or @($checked.results | Where-Object { !$_.passed }).Count) {
            throw 'Timing receipt custody incomplete'
        }
    }
}
exit $code
