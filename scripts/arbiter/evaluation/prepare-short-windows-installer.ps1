# SPDX-License-Identifier: Apache-2.0
# Retire six superseded native image copies and relocate only the original stock source tree.
[CmdletBinding()]
param()
$ErrorActionPreference='Stop'
$workspace=(Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$campaign=(Resolve-Path (Join-Path $workspace 'artifacts/v190/final-plan/exec-20261116T052942Z')).Path
$old=(Resolve-Path (Join-Path $campaign 'iv')).Path
$parent=(Resolve-Path (Join-Path $workspace 'artifacts/v190/final-plan')).Path
$new=Join-Path $parent 'iv'
if((Split-Path -Parent $old) -ne $campaign -or (Split-Path -Parent $new) -ne $parent -or (Test-Path -LiteralPath $new)){
    throw 'Original private source view or fresh short path differs'
}
$sid='S-1-5-21-3678370870-3858642417-826968105-1002'
. (Join-Path $workspace 'scripts/dev/assert-human-execution.ps1')
$human=Assert-AirkHumanExecution $sid
function Checked($p){$null=Assert-AirkCreatedOutput $p $workspace $sid}
function Before($p){$null=Assert-AirkOutputParent $p $workspace $sid}
function Exact($p,$sha){Checked $p;if((Get-FileHash -LiteralPath $p).Hash -ne $sha){throw "Changed $p"}}
$owner=Join-Path $campaign 'windows-native-local-installer-r1/execution.json'
$compile=Join-Path $old 'artifacts/local-service-installer/20261120-055958'
$control=Join-Path $campaign 'windows-iscc-long-path-controls-r1/receipt.json'
Exact $owner 'E366FD99C751812B5CFB6CDCB41BB6FEE0F27B12E0A74D910160489BE3B7EF5B'
Exact (Join-Path $compile 'receipt.json') 'E0A1C67B9BC2FF5C67FC61F9B18C2D5AB7DD515BADF6621D777C952EC2F3AB85'
Exact $control '04456BCBF3DE77E4C44B4D34B1ADDF4508693F2BAB6D5864329C746839787C6A'
$job=(Get-Content $owner -Raw|ConvertFrom-Json).job
if($job.exit_code -ne 1 -or $job.owned_job_final_active_processes -ne 0 -or !$job.owned_descendants_settled -or $job.owned_job_forced_termination){throw 'Owned stock failure has not settled'}
$native=@{'airk.exe'='83066297a152122b2ba83daf7fdf21b3c428b2b3024b6c73947efd8ef5bec6fd';'airkctl.exe'='05e7a16235f4b337b2ec6670fa936873fa6746671dfee196e6ac7730abe7d945'}
foreach($name in $native.Keys){Exact (Join-Path (Join-Path $campaign 'windows-fresh-cpu-product-r2/runtime') $name) $native[$name]}
$roster=[Collections.Generic.List[object]]::new()
$dirs=0
foreach($area in @($old,(Join-Path $campaign 'windows-native-local-installer-r1'))){
    $folders=[Collections.Generic.Stack[IO.DirectoryInfo]]::new()
    $folders.Push((Get-Item -LiteralPath $area))
    while($folders.Count){
        $d=$folders.Pop()
        if($d.Attributes -band [IO.FileAttributes]::ReparsePoint){throw "Redirected $d"}
        $acl=[IO.FileSystemAclExtensions]::GetAccessControl($d)
        if($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid){throw "Foreign directory $d"}
        $null=Assert-AirkDirectoryPolicy $acl $human
        $dirs++
        foreach($f in $d.EnumerateFileSystemInfos()){
            if($f.Attributes -band [IO.FileAttributes]::ReparsePoint){throw "Redirected $f"}
            if($f -is [IO.DirectoryInfo]){$folders.Push($f);continue}
            $acl=[IO.FileSystemAclExtensions]::GetAccessControl($f)
            if($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid){throw "Foreign file $f"}
            $h=[IO.File]::Open($f.FullName,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::None)
            try{$sha=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($h)).ToLowerInvariant()}finally{$h.Dispose()}
            $roster.Add(@{path=$f.FullName;sha256=$sha;bytes=$f.Length})
        }
    }
}
$retired=@()
foreach($name in $native.Keys){
    $placed=if($name -eq 'airk.exe'){'service'}else{'bin'}
    foreach($relative in @("tested-runtime/$name","qualified-runtime/$name","layout/$placed/$name")){
        $file=(Resolve-Path -LiteralPath (Join-Path $compile $relative)).Path
        if(!$file.StartsWith($compile+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Unscoped generated image'}
        Exact $file $native[$name]
        $retired+=$file
    }
}
$targets=@(Get-ChildItem -LiteralPath $old -Force|Where-Object Name -ne 'artifacts')
$proof=Join-Path $campaign 'windows-stock-installer-r1-disposition-custody.json'
Before $proof
if(Test-Path -LiteralPath $proof){throw 'Preserve original disposition'}
[IO.File]::WriteAllText($proof,(@{owner_sid=$sid;roster=$roster;directories=$dirs;original_root=$old;new_root=$new;
    moved=@($targets|ForEach-Object Name);superseded_images=$retired;original_job_sha256=(Get-FileHash $owner).Hash}|
    ConvertTo-Json -Compress -Depth 5))
Checked $proof
Before $new
$null=New-Item -ItemType Directory -Path $new
Checked $new
foreach($item in $targets){
    $src=(Resolve-Path -LiteralPath $item.FullName).Path
    $dst=[IO.Path]::GetFullPath((Join-Path $new $item.Name))
    if((Split-Path -Parent $src) -ne $old -or (Split-Path -Parent $dst) -ne $new -or (Test-Path -LiteralPath $dst)){
        throw "Rejected source transfer $src"
    }
    Before $dst
    Move-Item -LiteralPath $src -Destination $dst
    Checked $dst
}
foreach($file in $retired){Exact $file $native[(Split-Path -Leaf $file)];Remove-Item -LiteralPath $file}
foreach($entry in $roster){
    if(!$entry.path.StartsWith($old+'\',[StringComparison]::OrdinalIgnoreCase)){continue}
    $relative=[IO.Path]::GetRelativePath($old,$entry.path)
    if($relative.StartsWith('artifacts\',[StringComparison]::OrdinalIgnoreCase)){continue}
    $target=Join-Path $new $relative
    if((Get-FileHash -LiteralPath $target).Hash -ne $entry.sha256){throw "Source transfer changed $relative"}
}
$closure=Join-Path $campaign 'windows-stock-installer-source-transfer-r2.json'
Before $closure
if(Test-Path -LiteralPath $closure){throw 'Source transfer receipt exists'}
[IO.File]::WriteAllText($closure,(@{before_sha256=(Get-FileHash $proof).Hash;old=$old;source=$new;owner_sid=$sid;
    superseded_image_count=$retired.Count;remaining_stock_receipt=(Get-FileHash (Join-Path $compile 'receipt.json')).Hash}|
    ConvertTo-Json -Compress))
Checked $closure
(Get-FileHash $closure).Hash
