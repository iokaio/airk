# SPDX-License-Identifier: Apache-2.0
# Transfer the three verified immutable inputs after the first fresh CPU configure stopped.
param([ValidateSet('quota','tool-path')][string]$Cause='quota')
$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$campaign = (Resolve-Path (Join-Path $workspace 'artifacts/v190/final-plan/exec-20261116T052942Z')).Path
$fromName,$toName,$epoch=if($Cause -eq 'quota'){@('o','p',1)}else{@('p','q',2)}
$original = (Resolve-Path (Join-Path $campaign $fromName)).Path
$fresh = [IO.Path]::GetFullPath((Join-Path $campaign $toName))
$report = Join-Path $campaign "windows-cpu-ort-input-transfer-r$epoch.json"
$sid = 'S-1-5-21-3678370870-3858642417-826968105-1002'
. (Join-Path $workspace 'scripts/dev/assert-human-execution.ps1')
$human = Assert-AirkHumanExecution $sid
$python = 'C:/Users/tdotn/AppData/Local/Programs/Python/Python313/python.exe'
$runner = Join-Path $PSScriptRoot 'windows_cpu_ort.py'
$names = @('downloads','source','source-files.json')
function Assert-Seed([string]$root,[bool]$exclusive) {
    $count=0
    $bytes=[long]0
    foreach($name in $names) {
        $item=Get-Item -LiteralPath (Join-Path $root $name)
        $items=@($item)
        if($item.PSIsContainer){$items+=@(Get-ChildItem -LiteralPath $item.FullName -Recurse -Force)}
        foreach($entry in $items) {
            if(($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0){throw "Reparse input $($entry.FullName)"}
            $acl=[IO.FileSystemAclExtensions]::GetAccessControl($entry)
            if($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid){throw "Foreign input $($entry.FullName)"}
            if($entry.PSIsContainer){$null=Assert-AirkDirectoryPolicy $acl $human}
            else{
                $bytes+=$entry.Length
                if($exclusive){$stream=[IO.File]::Open($entry.FullName,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::None);$stream.Dispose()}
            }
            $count++
        }
    }
    return [ordered]@{entries=$count;file_bytes=$bytes}
}
function Verify-Original([string]$name){
    $raw=& $python -I -B -S $runner --verify-seed $name
    if($LASTEXITCODE -ne 0){throw "Stock verifier rejected $name"}
    return ($raw|ConvertFrom-Json -AsHashtable)
}
if([IO.Path]::GetDirectoryName($original) -ne $campaign -or [IO.Path]::GetDirectoryName($fresh) -ne $campaign -or
   (Test-Path -LiteralPath $fresh) -or (Test-Path -LiteralPath $report)){throw 'Fresh sibling paths required'}
$failed=Join-Path $original 'build-attempt.json'
$attempt=Get-Content -LiteralPath $failed -Raw | ConvertFrom-Json
$outer=Join-Path $campaign "windows-fresh-cpu-ort-r$epoch/execution.json"
$ended=Get-Content -LiteralPath $outer -Raw|ConvertFrom-Json
if($Cause -eq 'quota' -and ($attempt.result -ne 'failed' -or $attempt.commands.Count -ne 1 -or $attempt.commands[0].exit_code -ne 1816 -or
   $ended.job.owned_job_final_active_processes -ne 0 -or !$ended.job.owned_descendants_settled -or
   $ended.job.owned_job_forced_termination -or (Test-Path -LiteralPath (Join-Path $original 'b')))){throw 'Wrong historical configure closure'}
if($Cause -eq 'tool-path'){
    $control=Join-Path $campaign 'windows-cpu-ort-lib-alias-control-r1/execution.json'
    if((Get-FileHash $outer).Hash -ne '216DAFF7D999AB08F981D739B1F5D520FC446D0A7B3AA16F38A03084C50E9F46' -or
       (Get-FileHash $failed).Hash -ne '526DA78B98A09EA1977CD8B544FA85162743E6941AA0386CD7A0EA29C6088DE1' -or
       (Get-FileHash $control).Hash -ne '0D686A48D6BA0EA300D4BBDB8CA59B79942F544DFF59B686B62190F5F8D46D59' -or
       $attempt.result -ne 'running' -or $attempt.commands.Count -ne 1 -or $attempt.commands[0].exit_code -ne 0 -or
       (Get-Process cl,lib,cmake,ninja -ErrorAction SilentlyContinue)){throw 'Wrong interrupted compiler or live native builders'}
}
$start=Verify-Original $fromName
$prior=Assert-Seed $original $true
$null=Assert-AirkOutputParent $fresh $campaign $sid
$null=New-Item -ItemType Directory -Path $fresh
$null=Assert-AirkCreatedOutput $fresh $campaign $sid
$owner=Join-Path $fresh 'owner.json'
$null=Assert-AirkOutputParent $owner $fresh $sid
[IO.File]::Copy((Join-Path $original 'owner.json'),$owner,$false)
$null=Assert-AirkCreatedOutput $owner $fresh $sid
foreach($name in $names){
    $source=(Resolve-Path -LiteralPath (Join-Path $original $name)).Path
    $destination=[IO.Path]::GetFullPath((Join-Path $fresh $name))
    if([IO.Path]::GetDirectoryName($source) -ne $original -or [IO.Path]::GetDirectoryName($destination) -ne $fresh -or
       (Test-Path -LiteralPath $destination)){throw 'Source or no-overwrite target escaped'}
    $null=Assert-AirkOutputParent $destination $fresh $sid
    Move-Item -LiteralPath $source -Destination $destination -ErrorAction Stop
    $null=Assert-AirkCreatedOutput $destination $fresh $sid
}
$end=Verify-Original $toName
$after=Assert-Seed $fresh $false
if($start.source_inventory -ne $end.source_inventory -or $start.locked_inputs -ne 16 -or $end.locked_inputs -ne 16 -or
   $prior.entries -ne $after.entries -or $prior.file_bytes -ne $after.file_bytes){throw 'Transferred immutable source differs'}
$receipt=[ordered]@{source=$original;destination=$fresh;entries=$names;pre=$start;post=$end;pre_ownership=$prior;
                   post_ownership=$after;owner_sid=$sid;failed_attempt_sha256=(Get-FileHash $failed).Hash;
                   original_job_sha256=(Get-FileHash $outer).Hash;source_sha256=(Get-FileHash $PSCommandPath).Hash}
$null=Assert-AirkOutputParent $report $campaign $sid
$file=[IO.File]::Open($report,[IO.FileMode]::CreateNew)
try{$writer=[IO.StreamWriter]::new($file);$writer.WriteLine(($receipt|ConvertTo-Json -Depth 6));$writer.Flush()}
finally{$file.Dispose()}
$null=Assert-AirkCreatedOutput $report $campaign $sid
(Get-FileHash $report).Hash
