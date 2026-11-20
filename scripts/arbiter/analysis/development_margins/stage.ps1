# SPDX-License-Identifier: Apache-2.0
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$workspace = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../..'))
$campaign = Join-Path $workspace 'artifacts/v190/final-plan/exec-20261116T052942Z'
$out = Join-Path $campaign 'compact-development-source-menus-r2'
$sid = 'S-1-5-21-3678370870-3858642417-826968105-1002'
. (Join-Path $workspace 'scripts/dev/assert-human-execution.ps1')
$null = Assert-AirkHumanExecution $sid
$owner = Join-Path $PSScriptRoot 'stage.py'
$layout = python -I -B -S $owner --layout | ConvertFrom-Json
if ($LASTEXITCODE -or $layout.directories.Count -ne 15 -or $layout.files.Count -ne 751) { throw 'Layout differs' }
foreach ($dir in @($out) + @($layout.directories | ForEach-Object { Join-Path $out $_ })) {
    $null = Assert-AirkOutputParent $dir $workspace $sid
    if (Test-Path -LiteralPath $dir) { throw 'Fresh destination required' }
    [void][IO.Directory]::CreateDirectory($dir)
    $null = Assert-AirkCreatedOutput $dir $workspace $sid
}
function Test-MenuPaths([bool]$created) {
    foreach ($dir in $layout.directories) { $null = Assert-AirkCreatedOutput (Join-Path $out $dir) $workspace $sid }
    for ($i=0; $i -lt $layout.files.Count; $i+=32) {
        $end = [Math]::Min($i+31, $layout.files.Count-1)
        $batch = @(foreach ($path in $layout.files[$i..$end]) {
            [pscustomobject]@{path=(Join-Path $out $path); root=$out; created=$created}
        })
        $audit = Assert-AirkOutputBatch $batch $sid 10
        if ($audit.expired -or @($audit.results | Where-Object { !$_.passed }).Count) { throw 'Output audit failed' }
    }
}
Test-MenuPaths $false
python -I -B -S $owner
if ($LASTEXITCODE) { throw 'Source projection failed' }
Test-MenuPaths $true
$files = @(Get-ChildItem -LiteralPath $out -Recurse -File -Force)
$directories = @(Get-ChildItem -LiteralPath $out -Recurse -Directory -Force)
if ($files.Count -ne 751 -or $directories.Count -ne 15) { throw 'Output census differs' }
$manifest = Join-Path $out 'manifest.private.json'
$observed = (Get-Content $manifest -Raw | ConvertFrom-Json).files
if ($observed.Count -ne 750) { throw 'Membership count differs' }
for ($i=0; $i -lt $observed.Count; $i++) {
    $file = $observed[$i]
    if ($file.path -cne $layout.files[$i]) { throw 'Ordered source path changed' }
    $path = Join-Path $out $file.path
    if ((Get-Item $path).Length -ne $file.bytes -or (Get-FileHash $path).Hash -ne $file.sha256) { throw 'Source bytes differ' }
}
(Get-FileHash $manifest).Hash
