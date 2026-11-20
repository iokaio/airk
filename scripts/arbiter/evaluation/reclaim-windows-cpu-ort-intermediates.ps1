# SPDX-License-Identifier: Apache-2.0
# Keep the complete verified Ninja-output manifest before reclaiming only the original object/PCH bytes.
$ErrorActionPreference='Stop'
$workspace=(Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$campaign=(Resolve-Path (Join-Path $workspace 'artifacts/v190/final-plan/exec-20261116T052942Z')).Path
$sid='S-1-5-21-3678370870-3858642417-826968105-1002'
. (Join-Path $workspace 'scripts/dev/assert-human-execution.ps1')
$null=Assert-AirkHumanExecution $sid
$build=(Resolve-Path (Join-Path $campaign 'q/b/Release')).Path
$report=Join-Path $campaign 'windows-cpu-ort-reclaimed-intermediates-r3.json'
$planner=Join-Path $PSScriptRoot 'windows_ort_intermediates.py'
$python='C:/Users/tdotn/AppData/Local/Programs/Python/Python313/python.exe'
if(Test-Path -LiteralPath $report){throw 'Original retained inventory already exists'}
if(Get-Process cl,lib,cmake,ninja,cargo,rustc -ErrorAction SilentlyContinue){throw 'Native compiler or Cargo target still active'}
$null=Assert-AirkOutputParent $report $campaign $sid
$raw=& $python -I -B -S $planner
if($LASTEXITCODE){throw 'Stock object inventory refused'}
$manifest=$raw|ConvertFrom-Json -AsHashtable
if($manifest.root -ne $build -or !$manifest.items.Count){throw 'Expected exact current q/b/Release artifacts'}
$manifest.planner_sha256=(Get-FileHash $planner).Hash
$manifest.reclaimer_sha256=(Get-FileHash $PSCommandPath).Hash
$manifest.owner_sid=$sid
$records=@(foreach($obj in $manifest.items){
    $path=(Resolve-Path -LiteralPath $obj.path).Path
    if($path -ne $obj.path -or !$path.StartsWith($build+'\',[StringComparison]::OrdinalIgnoreCase) -or
        ([IO.Path]::GetExtension($path) -notin @('.obj','.pch'))){throw 'Foreign reclaim scope'}
    [pscustomobject]@{path=$path;root=$build;created=$true}
})
for($i=0;$i -lt $records.Count;$i+=64){
    $last=[Math]::Min($i+63,$records.Count-1)
    $result=Assert-AirkOutputBatch -Entries $records[$i..$last] -ExpectedSid $sid -BudgetSeconds 10
    if($result.expired -or @($result.results|Where-Object {!$_.passed}).Count){throw 'Original object custody failed'}
}
$s=[IO.File]::Open($report,[IO.FileMode]::CreateNew)
try{$s.Write([Text.Encoding]::UTF8.GetBytes(($manifest|ConvertTo-Json -Depth 5)))}finally{$s.Dispose()}
$null=Assert-AirkCreatedOutput $report $campaign $sid
foreach($obj in $manifest.items){
    $path=(Resolve-Path -LiteralPath $obj.path).Path
    if(!$path.StartsWith($build+'\',[StringComparison]::OrdinalIgnoreCase) -or $path -ne $obj.path -or
       (Get-FileHash -LiteralPath $path).Hash -ne $obj.sha256){throw 'Original object changed after inventory'}
    Remove-Item -LiteralPath $path -ErrorAction Stop
}
foreach($obj in $manifest.items){if(Test-Path -LiteralPath $obj.path){throw 'Original named intermediate still present'}}
& (Join-Path $PSScriptRoot 'audit-windows-cpu-ort.ps1') -AfterReclaim
(Get-FileHash $report).Hash
