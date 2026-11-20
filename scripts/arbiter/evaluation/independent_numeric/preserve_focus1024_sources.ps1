# SPDX-License-Identifier: Apache-2.0
# Project the actual native build inputs into a stable, same-human source directory.
param([Parameter(Mandatory)][string]$BuildReceipt,
      [Parameter(Mandatory)][string]$BuildSha256,
      [Parameter(Mandatory)][string]$InputManifest,
      [Parameter(Mandatory)][string]$InputSha256)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::TrimEndingDirectorySeparator((Resolve-Path (Join-Path $PSScriptRoot '../../../../')).Path)
. (Join-Path $repo 'scripts/dev/assert-human-execution.ps1')
$sid = 'S-1-5-21-3678370870-3858642417-826968105-1002'
$null = Assert-AirkHumanExecution $sid
function Existing($p){$null=Assert-AirkCreatedOutput $p $repo $sid}
function Fresh($p){$null=Assert-AirkOutputParent $p $repo $sid;if(Test-Path -LiteralPath $p){throw "fresh output required $p"}}
function ReadPinned($p,$sha) {
    Existing $p
    $bytes=[IO.File]::ReadAllBytes($p)
    if([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)) -ne $sha){throw "pin mismatch $p"}
    [Text.Encoding]::UTF8.GetString($bytes) | ConvertFrom-Json -AsHashtable
}
$base=[IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($BuildReceipt))
$campaign=(Resolve-Path (Join-Path $repo 'artifacts/v190/final-plan/exec-20261116T052942Z')).Path
if([IO.Path]::GetDirectoryName($base) -ne $campaign -or [IO.Path]::GetDirectoryName($InputManifest) -ne $base){throw 'expected coordinated build and source manifest'}
$build=ReadPinned $BuildReceipt $BuildSha256
$manifest=ReadPinned $InputManifest $InputSha256
if(!$build.passed -or !$build.inputs_unchanged -or $manifest.native_build_receipt.sha256 -ne $BuildSha256 -or $manifest.repository_root -ne $repo){throw 'manifest/build identity changed'}
$snapshot=Join-Path $base 'focus-native-source-snapshot-r2'
$output=Join-Path $base 'source-manifest.json'
Fresh $snapshot
Fresh $output
$null=New-Item -Type Directory -Path $snapshot
Existing $snapshot
foreach($row in $manifest.source_files){
    $source=[IO.Path]::GetFullPath($row.path)
    Existing $source
    $relative=[IO.Path]::GetRelativePath($repo,$source).Replace('\','/')
    if(([IO.Path]::IsPathRooted($relative) -or $relative.StartsWith('../')) -or $build.groups.native.files[$relative] -ne $row.sha256){throw "unbound source $source"}
    $dest=$snapshot
    $parts=$relative.Split('/')
    for($i=0;$i -lt $parts.Count-1;$i++){
        $part=$parts[$i]
        $dest=Join-Path $dest $part
        if(!(Test-Path -LiteralPath $dest)){Fresh $dest;$null=New-Item -Type Directory -Path $dest}
        Existing $dest
    }
    $dest=Join-Path $dest ([IO.Path]::GetFileName($source))
    Fresh $dest
    [IO.File]::Copy($source,$dest,$false)
    Existing $dest
    if((Get-FileHash -LiteralPath $dest).Hash -ne $row.sha256){throw "source copy differs from exact build $relative"}
    $row.path=$dest
}
$manifest.repository_root=$snapshot
[IO.File]::WriteAllText($output,($manifest | ConvertTo-Json -Depth 32)+[char]10)
Existing $output
(Get-FileHash -LiteralPath $output).Hash.ToLowerInvariant()
