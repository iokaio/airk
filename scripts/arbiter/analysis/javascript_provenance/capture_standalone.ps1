# SPDX-License-Identifier: Apache-2.0
param([Parameter(Mandatory)][string]$ReportPath,[Parameter(Mandatory)][string]$ReportSha256,[switch]$Write,[switch]$SourcePolicySupplement,[switch]$FixtureSupplement)
$ErrorActionPreference='Stop'
if($SourcePolicySupplement -and $FixtureSupplement){throw 'Select one supplement scope'}
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../..'))
$sid='S-1-5-21-3678370870-3858642417-826968105-1002'
. (Join-Path $repo 'scripts/dev/assert-human-execution.ps1')
$identity=Assert-AirkHumanExecution -ExpectedSid $sid
$utf8=[Text.UTF8Encoding]::new($false,$true)
function WriteFresh([string]$Path,[byte[]]$Bytes){
    Assert-AirkOutputPathObservation -Path $Path -Root $repo -Identity $identity | Out-Null
    $stream=[IO.FileStream]::new($Path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
    try{$stream.Write($Bytes,0,$Bytes.Length)}finally{$stream.Dispose()}
    Assert-AirkOutputPathObservation -Path $Path -Root $repo -Identity $identity | Out-Null
    if(!(Test-Path -LiteralPath $Path -PathType Leaf)){throw 'Created source absent'}
    if((Get-Acl -LiteralPath $Path).GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid){throw 'Created source owner differs'}
}
function Digest([byte[]]$Bytes){[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($Bytes)).ToLowerInvariant()}
$ReportPath=Get-AirkCheckedPath ([IO.Path]::GetFullPath($ReportPath)) $repo
$reportBytes=[IO.File]::ReadAllBytes($ReportPath)
if((Digest $reportBytes) -cne $ReportSha256){throw 'Analysis report digest differs'}
$report=$utf8.GetString($reportBytes)|ConvertFrom-Json
if($report.schema -cne 'airk.private.javascript-provenance-analysis/1'){throw 'Unexpected analysis report'}
$family='scripts/arbiter/research/javascript_history_r1'
$rows=[Collections.Generic.List[object]]::new()
$copies=@{}
foreach($group in $report.groups){
    if($group.status -cne 'unresolved'){continue}
    foreach($source in $group.paths){
        if($FixtureSupplement){
            if($group.sha256 -cnotin @('063f34e1d8c08d7afc75b7ec7037c63b606f91108bdbaacec64c4cfd1673f668','76441669d7c34ec287cd434e9a8fe4793739735c26f96cb8804f9d1cef79ef48','0da807f83642e0c690e8d006e06c741e2c574f32d2ebd0fed14e8ff7cb6dead7','ad24e64154df1f5d74ece2e573884f9ae220aef6d2f7150d6ee64fec13858434') -or !$source.EndsWith('/out/serviceExtension.js')){continue}
        }elseif($SourcePolicySupplement){
            if($group.sha256 -cne 'b39d1a6a57aa8436ca8785b87280b39e160398239b6bfd7b827c2325d3e619ae' -or !$source.EndsWith('/out/sourcePolicy.js')){continue}
        }elseif(!($source.EndsWith('/driver.js') -or $source.EndsWith('/driver/test.js') -or $source -ceq 'artifacts/advanced-split/help-source-preview-b84fe5bd/search.js')){continue}
        if(!$source.StartsWith('artifacts/') -or $source.Split('/') -contains '..'){throw 'Source outside retained artifact scope'}
        $absolute=[IO.Path]::GetFullPath((Join-Path $repo $source))
        if(!$absolute.StartsWith($repo+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Source path escaped repository'}
        $absolute=Get-AirkCheckedPath $absolute $repo
        $item=Get-Item -LiteralPath $absolute
        if(($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0){throw 'Reparse source refused'}
        $raw=[IO.File]::ReadAllBytes($absolute)
        if($raw.Length -ne $group.bytes -or (Digest $raw) -cne $group.sha256){throw "Original source drift: $source"}
        $text=$utf8.GetString($raw)
        if($text -match '-----BEGIN (RSA |OPENSSH |EC )?PRIVATE KEY-----|\bgh[pousr]_[A-Za-z0-9]{30,}|\bAKIA[A-Z0-9]{16}\b|\bhf_[A-Za-z0-9]{25,}'){throw 'Potential secret requires review'}
        $prefix=''
        if(($text -split "`n",2)[0] -notmatch 'SPDX-License-Identifier: Apache-2.0'){$prefix="// SPDX-License-Identifier: Apache-2.0`n"}
        [byte[]]$kept=[Text.Encoding]::UTF8.GetBytes($prefix)+$raw
        $destination="$family/sources/$($group.sha256)--$([IO.Path]::GetFileName($source)).in"
        if($copies.ContainsKey($destination)){
            if((Digest $copies[$destination]) -cne (Digest $kept)){throw 'Source destination collision'}
        }else{$copies[$destination]=$kept}
        $rows.Add([ordered]@{source=$source;original_sha256=$group.sha256;original_bytes=$raw.Length;destination=$destination;maintained_sha256=(Digest $kept);maintained_bytes=$kept.Length;transformations=@($(if($prefix){[ordered]@{kind='spdx-prefix';prefix=$prefix}}))})
    }
}
$duplicates=@($rows | Group-Object { $_.source } | Where-Object Count -gt 1)
if($duplicates.Count){throw 'Duplicate source alias'}
$map=[ordered]@{schema='airk.source-relocation/1';input_analysis=[ordered]@{path=$ReportPath;sha256=$ReportSha256;bytes=$reportBytes.Length};scope=$(if($FixtureSupplement){'Four exact historical authored editor fixture implementations; no runtime data'}elseif($SourcePolicySupplement){'Exact historical AIRK sourcePolicy implementation gap; no vendor code'}else{'Unresolved standalone AIRK editor driver/test and help-search source only'});qualification=$false;historical_code_executed=$false;files=@($rows)}
if($Write){
    $mapName=if($FixtureSupplement){'fixtures/source-map.json'}elseif($SourcePolicySupplement){'source_policy/source-map.json'}else{'source-map.json'}
    $mapPath=Join-Path $repo "$family/$mapName"
    if(Test-Path -LiteralPath $mapPath){throw 'Fresh map already exists'}
    foreach($destination in $copies.Keys){if(Test-Path -LiteralPath (Join-Path $repo $destination)){throw 'Fresh source copy already exists'}}
    foreach($destination in $copies.Keys){
        $target=Join-Path $repo $destination
        WriteFresh $target $copies[$destination]
    }
    WriteFresh $mapPath ($utf8.GetBytes(($map|ConvertTo-Json -Depth 10)+"`n"))
}
[ordered]@{paths=$rows.Count;unique_copies=$copies.Count;wrote=[bool]$Write;qualification=$false}|ConvertTo-Json -Compress
