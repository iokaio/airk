# SPDX-License-Identifier: Apache-2.0
# Authenticate the original closed producer, audit all generated outputs, and independently replay stock --verify.
param([switch]$AfterReclaim)
$ErrorActionPreference='Stop'
$workspace=(Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$campaign=(Resolve-Path (Join-Path $workspace 'artifacts/v190/final-plan/exec-20261116T052942Z')).Path
$sid='S-1-5-21-3678370870-3858642417-826968105-1002'
. (Join-Path $workspace 'scripts/dev/assert-human-execution.ps1')
$human=Assert-AirkHumanExecution $sid
$builder=Join-Path $campaign 'q'
$run=Join-Path $campaign 'windows-fresh-cpu-ort-r3'
$report=Join-Path $campaign $(if($AfterReclaim){'windows-cpu-ort-post-reclaim-r3.json'}else{'windows-cpu-ort-custody-r3.json'})
$stock=Join-Path $workspace 'scripts/provision-static-onnx-runtime.py'
$python='C:/Users/tdotn/AppData/Local/Programs/Python/Python313/python.exe'
$null=Assert-AirkOutputParent $report $campaign $sid
if(Test-Path -LiteralPath $report){throw 'Output already exists'}
$pins=[ordered]@{}
function Pin([string]$path){
    $pins[$path]=(Get-FileHash -LiteralPath $path).Hash
}
$execution=Join-Path $run 'execution.json'
$closed=Get-Content -LiteralPath $execution -Raw|ConvertFrom-Json
$job=$closed.job
if(!$closed.passed -or $job.exit_code -ne 0 -or $job.timed_out -or !$job.owned_descendants_settled -or
   $job.owned_job_forced_termination -or $job.owned_job_final_active_processes -ne 0 -or
   $null -eq $job.owned_job_final_member_pids -or $job.owned_job_final_member_pids.Count -ne 0){throw 'Original successful closed Job required'}
$toolchain=(Get-Content -LiteralPath (Join-Path $builder 'build-receipt.json') -Raw|ConvertFrom-Json).toolchain
$probes=@($toolchain.cmake.probe.log,$toolchain.ninja.probe.log)|ForEach-Object {[IO.Path]::GetDirectoryName($_)}
$historic=[Collections.Generic.List[object]]::new()
$inventory=foreach($dir in @($builder,$run)){
    $count=0
    $bytes=[long]0
    $pending=[Collections.Generic.Stack[IO.DirectoryInfo]]::new()
    $pending.Push((Get-Item -LiteralPath $dir))
    while($pending.Count){
        $folder=$pending.Pop()
        if($folder.Attributes -band [IO.FileAttributes]::ReparsePoint){throw "Reparse directory $folder"}
        $acl=[IO.FileSystemAclExtensions]::GetAccessControl($folder)
        if($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid){throw "Foreign directory $folder"}
        try{$null=Assert-AirkDirectoryPolicy $acl $human}catch{
            if($folder.FullName -notin $probes -or $_.Exception.Message -ne 'No explicit human full-control grant covering parent and inherited outputs'){throw}
            $dacl=$acl.GetSecurityDescriptorSddlForm([Security.AccessControl.AccessControlSections]::Access)
            if($dacl -ne 'D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)'){throw 'Different private tool probe ACL'}
            $files=@($folder.EnumerateFileSystemInfos())
            if($files.Count -ne 2 -or @($files|Where-Object {$_.Name -notin @('output.log','result.json') -or $_ -is [IO.DirectoryInfo]}).Count){throw 'Unexpected historical tool-probe entries'}
            $historic.Add([ordered]@{path=$folder.FullName;owner_sid=$sid;dacl=$dacl;files=@($files.Name);future_writable=$false;historical_guard_refusal=$_.Exception.Message})
        }
        foreach($entry in $folder.EnumerateFileSystemInfos()){
            if($entry.Attributes -band [IO.FileAttributes]::ReparsePoint){throw "Reparse output $entry"}
            if($entry -is [IO.DirectoryInfo]){$pending.Push($entry);continue}
            $fileAcl=[IO.FileSystemAclExtensions]::GetAccessControl($entry)
            if($fileAcl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid){throw "Foreign file $entry"}
            $bytes+=$entry.Length
            $count++
        }
    }
    [ordered]@{root=$dir;files=$count;bytes=$bytes}
}
$cmd=Get-Content -LiteralPath (Join-Path $run 'producer-command.json') -Raw|ConvertFrom-Json
if((Get-FileHash -LiteralPath $stock).Hash -ne $cmd.producer_sha256){throw 'Stock producer changed'}
foreach($path in @($stock,$PSCommandPath,$execution,(Join-Path $run 'cpu-environment.json'),
    (Join-Path $builder 'build-receipt.json'),(Join-Path $builder 'static-aggregate-receipt.json'))){Pin $path}
$env:TMPDIR=$env:TEMP=$env:TMP=(Join-Path $run 'temp')
$trace=Join-Path $PSScriptRoot 'verify_windows_cpu_ort_readonly.py'
Pin $trace
$argv=@('-I','-B','-S',$trace)
$raw=& $python @argv
if($LASTEXITCODE -ne 0){throw 'Fresh stock --verify failed'}
$observation=($raw -join [Environment]::NewLine)|ConvertFrom-Json -AsHashtable
$checked=$observation.result
if($historic.Count -ne 2 -or $observation.opened_excluded -ne 0 -or $observation.write_requests -ne 0 -or
   (Compare-Object ($historic.path|Sort-Object) ($observation.excluded_historic_directories|Sort-Object))){throw 'Read-only stock inputs did not exclude the complete historical refusal set'}
$originalJson=Get-Content -LiteralPath (Join-Path $run 'cpu-environment.json') -Raw
if(![System.Text.Json.Nodes.JsonNode]::DeepEquals([System.Text.Json.Nodes.JsonNode]::Parse(($checked|ConvertTo-Json -Depth 100 -Compress)),[System.Text.Json.Nodes.JsonNode]::Parse($originalJson))){throw 'Fresh stock result differs'}
foreach($pair in $pins.GetEnumerator()){if((Get-FileHash -LiteralPath $pair.Key).Hash -ne $pair.Value){throw "Changed input $($pair.Key)"}}
$result=[ordered]@{owner_sid=$sid;inventories=$inventory;inputs=$pins;argv=@($python)+$argv;environment=$checked.environment;
    historic_unwritable=$historic;verified_read_count=$observation.unique_read_paths;read_set_sha256=$observation.read_set_sha256;opened_excluded=0;write_requests=0}
$bytes=[Text.Encoding]::UTF8.GetBytes(($result|ConvertTo-Json -Depth 16))
$file=[IO.File]::Open($report,[IO.FileMode]::CreateNew)
try{$file.Write($bytes)}finally{$file.Dispose()}
$null=Assert-AirkCreatedOutput $report $campaign $sid
(Get-FileHash -LiteralPath $report).Hash
