# SPDX-License-Identifier: Apache-2.0
# Seed original immutable dependencies, then exercise real offline Cargo dependency resolution.
$ErrorActionPreference='Stop'
$workspace=(Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$campaign=(Resolve-Path (Join-Path $workspace 'artifacts/v190/final-plan/exec-20261116T052942Z')).Path
$sid='S-1-5-21-3678370870-3858642417-826968105-1002'
. (Join-Path $workspace 'scripts/dev/assert-human-execution.ps1')
$human=Assert-AirkHumanExecution $sid
$shared=Join-Path $campaign 'windows-offline-cargo-r1'
$cache=Join-Path $shared 'home'
$source='C:\Users\tdotn\.cargo'
$python='C:/Users/tdotn/AppData/Local/Programs/Python/Python313/python.exe'
$planner=Join-Path $PSScriptRoot 'windows_cargo_seed.py'
$envOwner=Join-Path $PSScriptRoot 'use-windows-offline-cargo.ps1'
function Check-Batch($paths,[bool]$created){
    $entries=@(foreach($p in $paths){[pscustomobject]@{path=$p;root=$campaign;created=$created}})
    for($i=0;$i -lt $entries.Count;$i+=64){
        $last=[Math]::Min($i+63,$entries.Count-1)
        $r=Assert-AirkOutputBatch -Entries $entries[$i..$last] -ExpectedSid $sid -BudgetSeconds 10
        if($r.expired -or @($r.results|Where-Object {!$_.passed}).Count){throw "Cargo cache custody failed at $i"}
    }
}
function Fresh-Dir($path){
    $null=Assert-AirkOutputParent $path $campaign $sid
    if([IO.Directory]::Exists($path)){throw "Fresh directory required: $path"}
    $null=[IO.Directory]::CreateDirectory($path)
    $null=Assert-AirkCreatedOutput $path $campaign $sid
}
function Snapshot-C(){
    $rows=[ordered]@{}
    foreach($f in Get-ChildItem -LiteralPath $source -Force -File | Where-Object {$_.Name -like '.global-cache*' -or $_.Name -like '.package-cache*'}){
        $rows[$f.Name]=@($f.Length,$f.LastWriteTimeUtc.Ticks,(Get-FileHash -LiteralPath $f.FullName).Hash)
    }
    return $rows
}
function Write-New($path,$body){
    $null=Assert-AirkOutputParent $path $campaign $sid
    $s=[IO.File]::Open($path,[IO.FileMode]::CreateNew)
    try{$s.Write([Text.Encoding]::UTF8.GetBytes($body))}finally{$s.Dispose()}
    $null=Assert-AirkCreatedOutput $path $campaign $sid
}
if((Get-Process cargo,rustc -ErrorAction SilentlyContinue)){throw 'An original Cargo/rustc still runs; exclusive first control required'}
$initial=Snapshot-C
$plan=(& $python -I -B -S $planner)|ConvertFrom-Json -AsHashtable
if($LASTEXITCODE -ne 0 -or $plan.packages -ne 436 -or $plan.files.Count -lt 436){throw 'Missing verified lockfile input'}
$scratch=Join-Path $shared 'temp'
if(Test-Path -LiteralPath $shared){
    foreach($dir in @($shared,$cache,$scratch)){$null=Assert-AirkCreatedOutput $dir $campaign $sid}
    $contents=@(Get-ChildItem -LiteralPath $shared -Recurse -Force)
    if($contents.Count -ne 2 -or @($contents|Where-Object {!$_.PSIsContainer -or $_.FullName -notin @($cache,$scratch)}).Count){throw 'Seed must be fresh or contain only the two checked empty bootstrap directories'}
}else{Fresh-Dir $shared;Fresh-Dir $cache;Fresh-Dir $scratch}
$files=@(foreach($k in $plan.files.Keys){[IO.Path]::GetFullPath((Join-Path $cache $k))})
$dirs=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
foreach($p in $files){
    $d=[IO.Path]::GetDirectoryName($p)
    while($d -ne $cache){if(!$d.StartsWith($cache+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Invalid seed path'}
        if(!$dirs.Add($d)){break};$d=[IO.Path]::GetDirectoryName($d)}
}
foreach($group in $dirs|Group-Object {($_ -split '\\').Count}|Sort-Object {[int]$_.Name}){
    Check-Batch $group.Group $false
    foreach($d in $group.Group){$null=[IO.Directory]::CreateDirectory($d)}
    Check-Batch $group.Group $true
}
Check-Batch $files $false
foreach($k in $plan.files.Keys){
    $old=Join-Path $source $k
    $new=Join-Path $cache $k
    [IO.File]::Copy($old,$new,$false)
    if((Get-FileHash -LiteralPath $new).Hash -ne $plan.files[$k].sha256){throw "Copy differs: $k"}
}
Check-Batch $files $true
$inventory=Join-Path $shared 'seed.json'
Write-New $inventory ($plan|ConvertTo-Json -Depth 5)
. $envOwner
Use-AirkOfflineCargo -Scratch $scratch -SeedProbe
$env:CARGO_TARGET_DIR=Join-Path $shared 'probe-target'
$null=Assert-AirkOutputParent $env:CARGO_TARGET_DIR $campaign $sid
$locked=[IO.File]::Open((Join-Path $source '.global-cache'),[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
$argv=@('tree','--locked','--offline','--target','x86_64-pc-windows-msvc','-p','airkctl','--depth','1','--prefix','none')
try{$actual=@(& $env:CARGO @argv);$code=$LASTEXITCODE}finally{$locked.Dispose()}
if($code -ne 0 -or $actual.Count -lt 2 -or $actual[0] -notmatch '^airkctl v'){throw "Original Cargo dependency resolution failed ($code)"}
$terminal=Snapshot-C
if(($initial|ConvertTo-Json -Compress) -ne ($terminal|ConvertTo-Json -Compress)){throw 'C Cargo DB or lock changed'}
if(!(Test-Path -LiteralPath (Join-Path $cache '.global-cache'))){throw 'Real D Cargo database missing'}
$bytes=[long]0;$size=[long]0;$count=0
foreach($entry in Get-ChildItem -LiteralPath $shared -Recurse -Force){
    if($entry.Attributes -band [IO.FileAttributes]::ReparsePoint){throw "Unexpected reparse: $entry"}
    $acl=[IO.FileSystemAclExtensions]::GetAccessControl($entry)
    if($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid){throw "Foreign output: $entry"}
    if($entry.PSIsContainer){$null=Assert-AirkDirectoryPolicy $acl $human;$size+=4096}
    else{$bytes+=$entry.Length;$size+=4096*[long][Math]::Ceiling($entry.Length/4096)}
    $count++
}
if($size -gt (1.5GB-1MB)){throw 'D shared cache exceeds the original allocation'}
$result=[ordered]@{passed=$true;root=$cache;owner=$sid;argv=@($env:CARGO)+$argv;cargo_sha256=(Get-FileHash $env:CARGO).Hash;
    c_before=$initial;c_after=$terminal;original_database_write_denied=$true;dependencies=$actual;entries=$count;bytes=$bytes;
    allocated_upper_bytes=$size;limit_bytes=1.5GB;lock_sha256=$plan.lock_sha256;inventory_sha256=(Get-FileHash $inventory).Hash;
    environment_owner=(Get-FileHash $envOwner).Hash;seed_owner=(Get-FileHash $PSCommandPath).Hash;planner=(Get-FileHash $planner).Hash}
$report=Join-Path $shared 'execution.json'
Write-New $report ($result|ConvertTo-Json -Depth 8)
(Get-FileHash $report).Hash
