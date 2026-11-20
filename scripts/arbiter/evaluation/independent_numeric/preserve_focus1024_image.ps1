# SPDX-License-Identifier: Apache-2.0
# Capture the actual Windows CU15 Cargo/PE/static linkage before releasing the shared target.
param([Parameter(Mandatory)][string]$BuildDirectory,
      [Parameter(Mandatory)][string]$BuildName,
      [Parameter(Mandatory)][string]$BuildSha256,
      [Parameter(Mandatory)][string]$CargoLogSha256,
      [Parameter(Mandatory)][string]$InferenceDestination)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../../../../')).Path
. (Join-Path $repo 'scripts/dev/assert-human-execution.ps1')
$sid = 'S-1-5-21-3678370870-3858642417-826968105-1002'
$identity = Assert-AirkHumanExecution $sid
$campaign = (Resolve-Path (Join-Path $repo 'artifacts/v190/final-plan/exec-20261116T052942Z')).Path
$base = (Resolve-Path -LiteralPath $BuildDirectory).Path
if ([IO.Path]::GetDirectoryName($base) -ne $campaign) { throw 'direct checked campaign build root required' }
$stage = (Resolve-Path (Join-Path $campaign 'windows-cu15-scientific-provider-stage-r1')).Path
if ([IO.Path]::GetDirectoryName($InferenceDestination) -ne $stage) { throw 'approved protected runtime stage required' }
function Existing($path) { $null = Assert-AirkCreatedOutput $path $repo $sid }
function Fresh($path) {
    $null = Assert-AirkOutputParent $path $repo $sid
    if (Test-Path -LiteralPath $path) { throw "fresh output required: $path" }
}
function Pin($path) {
    $file = Get-Item -LiteralPath $path
    if (($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $file.PSIsContainer) { throw 'regular file required' }
    [ordered]@{path=$file.FullName;bytes=$file.Length;sha256=(Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant()}
}
function Save($name,$value) {
    $path = Join-Path $base $name
    Fresh $path
    [IO.File]::WriteAllText($path, ($value | ConvertTo-Json -Depth 40) + [char]10, [Text.UTF8Encoding]::new($false))
    Existing $path
}
function Machine($file) {
    $stream = [IO.File]::OpenRead($file)
    try {
        $reader = [IO.BinaryReader]::new($stream)
        if ($reader.ReadUInt16() -ne 0x5a4d) { throw 'expected DOS image' }
        $stream.Position=60; $offset=$reader.ReadUInt32()
        if ($offset -lt 64 -or $offset -gt 1MB) { throw 'invalid PE offset' }
        $stream.Position=$offset
        if($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne 0x8664) { throw 'Windows x64 PE required' }
        $stream.Position=$offset+24
        if($reader.ReadUInt16() -ne 0x20b) { throw 'PE32+ required' }
        [ordered]@{machine_u16=0x8664;optional_header_magic_u16=0x20b}
    } finally { $stream.Dispose() }
}
$receiptPath = Join-Path $base ($BuildName+'.json')
$logPath = Join-Path $base ($BuildName+'.log')
Existing $receiptPath; Existing $logPath
$buildPin=Pin $receiptPath; $logPin=Pin $logPath
$build=Get-Content -LiteralPath $receiptPath -Raw | ConvertFrom-Json -AsHashtable
if($buildPin.sha256 -ne $BuildSha256 -or !$build.passed -or !$build.inputs_unchanged -or
   $build.changed_groups.Count -ne 0 -or $build.exit_code -ne 0 -or $build.runner_exit_code -ne 0 -or
   $build.log -ne ($BuildName+'.log') -or $CargoLogSha256 -ne $logPin.sha256 -or $build.command -notcontains '--no-run' -or
   $build.command -notcontains '--message-format=json') { throw 'successful unchanged Cargo JSON input required' }
$events=@(Get-Content -LiteralPath $logPath | Where-Object {$_.StartsWith('{')} | ForEach-Object {$_ | ConvertFrom-Json -AsHashtable})
$finished=@($events|Where-Object {$_.reason -eq 'build-finished'})
if($finished.Count -ne 1 -or !$finished[0].success) {throw 'Cargo did not finish once'}
$environmentPath='C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z/cu15/environment.json'
$environment=Pin $environmentPath
if($environment.sha256 -ne '05c074577ed80c5f2ac733a5775f71732ff631c890a78932083fddde9a7b4d5c'){throw 'CU15 environment changed'}
$envDoc=Get-Content -LiteralPath $environmentPath -Raw|ConvertFrom-Json -AsHashtable
$library=$envDoc.environment.ORT_LIB_LOCATION
$archive=Pin (Join-Path $library 'onnxruntime.lib')
if($archive.sha256 -ne $envDoc.aggregate.output.sha256 -or $archive.bytes -ne $envDoc.aggregate.output.bytes){throw 'CU15 archive changed'}
$ort=@($events|Where-Object {$_.reason -eq 'build-script-executed' -and $_.package_id.EndsWith('#ort-sys@2.0.0-rc.10')})
if($ort.Count -ne 1 -or ($ort[0].linked_libs -join ',') -ne 'static=onnxruntime' -or
   ($ort[0].linked_paths -join ',') -ne "native=$library"){throw 'wrong static linkage'}
$images=@();$bindings=@()
foreach($name in @('airk_inference','airk_service','airk_retrieval')){
    $rows=@($events|Where-Object {$_.reason -eq 'compiler-artifact' -and $_.target.name -eq $name -and $_.profile.test -and $_.executable})
    if($rows.Count -ne 1){throw 'ambiguous library test image'}
    $row=$rows[0]; $original=[IO.Path]::GetFullPath($row.executable)
    if([IO.Path]::GetDirectoryName($original) -ne (Join-Path $build.target_directory 'debug/deps')){throw 'original image outside expected target'}
    Existing $original
    if ([DateTimeOffset](Get-Item -LiteralPath $original).LastWriteTimeUtc -gt [DateTimeOffset]::FromUnixTimeMilliseconds([long]($build.ended_unix * 1000))) {
        throw 'Cargo artifact has been replaced after completion'
    }
    $before=Pin $original
    $pe=Machine $original
    $out=if($name -eq 'airk_inference'){$InferenceDestination}else{Join-Path $base ($name+'.exe')}
    Fresh $out
    [IO.File]::Copy($original,$out,$false)
    Existing $out
    $copy=Pin $out
    if($copy.sha256 -ne $before.sha256 -or (Pin $original).sha256 -ne $before.sha256){throw 'materialized image changed'}
    $image=[ordered]@{binary_id=$name.Replace('_','-');path=$copy.path;bytes=$copy.bytes;sha256=$copy.sha256;
        original_path=$original;original_file_owner_sid=$sid;preserved_file_owner_sid=$sid}
    $images+=,$image
    $bindings+=,[ordered]@{materialized_image=$image;actual_cargo_artifact=$row;pe=$pe}
}
$inventory=[ordered]@{schema='airk.preserved-native-test-images/1';utc=[DateTime]::UtcNow.ToString('o');
    source_commit=(git -C $repo rev-parse HEAD);native_group=$build.groups.native.sha256;actual_build_receipt=[ordered]@{path=$buildPin.path;sha256=$buildPin.sha256};
    images=$images;source_dirty=[bool](git -C $repo status --porcelain --untracked-files=normal);scope='Actual Cargo library images, static linkage, original and retained same-human files.'}
Save 'native-image-inventory.json' $inventory
$binding=[ordered]@{schema='airk.private-current-image-preservation-binding/1';inventory=(Pin (Join-Path $base 'native-image-inventory.json'));
    build_receipt=$buildPin;cargo_json=$logPin;actual_static_ORT_rows=$ort;static_archive=$archive;static_environment=$environment;images=$bindings;
    testcases_listed_or_run=$false;NN_invocations=0;preserver_source=(Pin $PSCommandPath)}
Save 'cargo-image-bindings.json' $binding
$binding.inventory.sha256

