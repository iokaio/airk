# SPDX-License-Identifier: Apache-2.0
param([Parameter(Mandatory)][string]$Analysis,[Parameter(Mandatory)][string]$Sha256,[Parameter(Mandatory)][string]$OutputRoot,[switch]$Write)
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../..'))
$sid='S-1-5-21-3678370870-3858642417-826968105-1002'
. (Join-Path $repo 'scripts/dev/assert-human-execution.ps1')
$identity=Assert-AirkHumanExecution -ExpectedSid $sid
$utf8=[Text.UTF8Encoding]::new($false,$true)
function HashBytes([byte[]]$bytes){[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()}
$Analysis=Get-AirkCheckedPath ([IO.Path]::GetFullPath($Analysis)) $repo
$OutputRoot=Get-AirkCheckedPath ([IO.Path]::GetFullPath($OutputRoot)) $repo
if(!$OutputRoot.StartsWith((Join-Path $repo 'artifacts')+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Private output root required'}
$raw=[IO.File]::ReadAllBytes($Analysis)
if((HashBytes $raw) -cne $Sha256){throw 'Analysis pin differs'}
$report=$utf8.GetString($raw)|ConvertFrom-Json
if($report.schema -cne 'airk.private.javascript-provenance-analysis/1'){throw 'Exact generation report required'}
$reportPin=[ordered]@{path=[IO.Path]::GetRelativePath($repo,$Analysis).Replace('\','/');sha256=$Sha256;bytes=$raw.Length}
$rows=[Collections.Generic.List[object]]::new();$seen=[Collections.Generic.HashSet[string]]::new()
foreach($group in $report.groups){
    if($group.status -cne 'proved_exact_generated_replica'){continue}
    $proof=$group.proof
    if($proof.kind -ceq 'retained_exact_generation_proof'){
        $generated=$proof.generated
        if($generated.reproduced_js_sha256 -cne $group.sha256){throw 'Retained output identity differs'}
    }elseif($proof.kind -ceq 'exact_in_memory_transpile'){
        $source=$proof.source
        if($source.path){$version=[ordered]@{path=$source.path;sha256=$source.sha256;bytes=$source.bytes}}
        elseif($source.git_blob_oid -and $source.git_head){$version=[ordered]@{git_blob_oid=$source.git_blob_oid;reachable_head=$source.git_head;path_label=$source.git_path_label;sha256=$source.sha256;bytes=$source.bytes}}
        else{throw 'Exact TypeScript source authority missing'}
        $generated=[ordered]@{compiler=$proof.compiler;source_version=$version;options=$proof.options;method=$proof.method;reproduced_js_sha256=$group.sha256;configuration=$proof.config;node=$report.node;producer=$report.owner}
    }else{throw 'Unknown proof kind cannot become exact disposition'}
    foreach($sourcePath in $group.paths){
        if(!$sourcePath.StartsWith('artifacts/') -or !$seen.Add($sourcePath)){throw 'Invalid or duplicate source path'}
        $rows.Add([ordered]@{source=$sourcePath;source_sha256=$group.sha256;source_bytes=$group.bytes;disposition='generated_from_maintained_typescript';reason='Exact retained or in-memory TypeScript emission matched whole output bytes; no runtime or historical build qualification.';generated=$generated;evidence=@($reportPin)})
    }
}
if($rows.Count -ne $report.proved_paths){throw 'Exact proof path count differs'}
$chunkSize=2000;$chunks=[int][Math]::Ceiling($rows.Count/[double]$chunkSize)
if($Write){
    if(!(Test-Path -LiteralPath $OutputRoot -PathType Container)){throw 'Output directory must be preguarded and created'}
    for($i=0;$i -lt $chunks;$i++){if(Test-Path -LiteralPath (Join-Path $OutputRoot "source-disposition-$i.json")){throw 'Fresh disposition already exists'}}
    for($i=0;$i -lt $chunks;$i++){
        $end=[Math]::Min($rows.Count,($i+1)*$chunkSize);$part=[Collections.Generic.List[object]]::new()
        for($j=$i*$chunkSize;$j -lt $end;$j++){$part.Add($rows[$j])}
        $document=[ordered]@{schema='airk.source-disposition/1';qualification=$false;files=@($part)}
        $bytes=$utf8.GetBytes(($document|ConvertTo-Json -Depth 30)+"`n")
        if($bytes.Length -gt 16MB){throw 'Disposition shard exceeds16MiB'}
        $target=Join-Path $OutputRoot "source-disposition-$i.json"
        Assert-AirkOutputPathObservation -Path $target -Root $repo -Identity $identity|Out-Null
        $stream=[IO.FileStream]::new($target,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
        try{$stream.Write($bytes,0,$bytes.Length)}finally{$stream.Dispose()}
        Assert-AirkOutputPathObservation -Path $target -Root $repo -Identity $identity|Out-Null
        if((Get-Acl -LiteralPath $target).GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid){throw 'Created disposition owner differs'}
    }
}
if((HashBytes ([IO.File]::ReadAllBytes($Analysis))) -cne $Sha256){throw 'Analysis drift during adaptation'}
[ordered]@{rows=$rows.Count;shards=$chunks;wrote=[bool]$Write;qualification=$false}|ConvertTo-Json -Compress
