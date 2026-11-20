# SPDX-License-Identifier: Apache-2.0
# Place each verified base directly under the native reader's isolated input directory.
[CmdletBinding()]
param()
$ErrorActionPreference='Stop'
$repo=(Resolve-Path (Join-Path $PSScriptRoot '../../../..')).Path
. (Join-Path $repo 'scripts/dev/assert-human-execution.ps1')
$sid='S-1-5-21-3678370870-3858642417-826968105-1002'
$null=Assert-AirkHumanExecution $sid
$campaign=Join-Path $repo 'artifacts/v190/final-plan/exec-20261116T052942Z'
$staged=Join-Path $campaign 'contextbench-six-private-corpora-r1'
$destination=Join-Path $campaign 'contextbench-six-native-inputs-r1'
$manifest=Join-Path $staged 'manifest.private.json'
$null=Assert-AirkCreatedOutput $manifest $repo $sid
if ((Get-FileHash $manifest).Hash -ne '46dede63b8ceb84464471a8d66846989184bf3a2b8525a5bef76278ed1a5a9b8') {
    throw 'Original private staging manifest changed'
}
if (Test-Path -LiteralPath $destination) { throw 'Fresh native input directory required' }
function Test-Paths([string[]]$paths, [string]$boundary, [bool]$created) {
    $observations=@(foreach($path in $paths){[pscustomobject]@{path=$path;root=$boundary;created=$created}})
    $audit=Assert-AirkOutputBatch $observations $sid 10
    if($audit.expired -or @($audit.results | Where-Object {!$_.passed}).Count) { throw 'Ownership audit failed' }
}
$contents=Get-Content $manifest -Raw | ConvertFrom-Json -AsHashtable
$rows=@($contents.staged | Where-Object area -eq 'sources')
if ($rows.Count -ne 13 -or @($rows.base | Sort-Object -Unique).Count -ne 6) { throw 'Source roster differs' }
$dirs=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$files=@(foreach ($r in $rows) {
    $source=Get-AirkCheckedPath (Join-Path $staged $r.relative_path) $staged
    $target=Get-AirkCheckedPath (Join-Path (Join-Path $destination $r.base) $r.path) $destination
    $parent=[IO.Path]::GetDirectoryName($target)
    while($parent -ne $destination){ $null=$dirs.Add($parent);$parent=[IO.Path]::GetDirectoryName($parent) }
    [pscustomobject]@{source=$source;target=$target;sha=$r.sha256}
})
$files+=@([pscustomobject]@{source=$manifest;target=(Join-Path $destination 'manifest.private.json');
                           sha='46dede63b8ceb84464471a8d66846989184bf3a2b8525a5bef76278ed1a5a9b8'})
Test-Paths $files.source $staged $true
foreach($f in $files){if((Get-FileHash $f.source).Hash -ne $f.sha){throw 'Input source changed'}}
$null=Assert-AirkOutputParent $destination $repo $sid
$null=New-Item -ItemType Directory -Path $destination
$null=Assert-AirkCreatedOutput $destination $repo $sid
foreach($level in ($dirs | Group-Object { ([IO.Path]::GetRelativePath($destination,$_)).Split('\').Length } | Sort-Object { [int]$_.Name })) {
    Test-Paths $level.Group $destination $false
    foreach($path in $level.Group){$null=New-Item -ItemType Directory -Path $path}
    Test-Paths $level.Group $destination $true
}
Test-Paths $files.target $destination $false
foreach($f in $files){[IO.File]::Copy($f.source,$f.target,$false)}
Test-Paths $files.target $destination $true
foreach($f in $files){if((Get-FileHash $f.target).Hash -ne $f.sha){throw 'Copied source changed'}}
if (@(Get-ChildItem -LiteralPath $destination -Recurse -File -Force).Count -ne $files.Count) { throw 'Output count differs' }
Write-Output 'Verified six isolated source revisions and 13 source files'
