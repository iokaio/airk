# SPDX-License-Identifier: Apache-2.0
# Audit the complete unsigned CPU installer and retire just its two verified transient package trees.
param([switch]$Reclaim)
$ErrorActionPreference='Stop'
$workspace=(Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$campaign=Join-Path $workspace 'artifacts/v190/final-plan/exec-20261116T052942Z'
$sid='S-1-5-21-3678370870-3858642417-826968105-1002'
. (Join-Path $workspace 'scripts/dev/assert-human-execution.ps1')
$human=Assert-AirkHumanExecution $sid
$owner=Join-Path $campaign 'windows-native-local-installer-r2'
$packages=(Resolve-Path -LiteralPath (Join-Path (Split-Path -Parent $campaign) 'iv/artifacts/local-service-installer')).Path
function Read-Json($p){Get-Content -LiteralPath $p -Raw|ConvertFrom-Json -AsHashtable}
function Checked($p){$null=Assert-AirkCreatedOutput $p $workspace $sid}
function Fresh($p){$null=Assert-AirkOutputParent $p $workspace $sid;if(Test-Path -LiteralPath $p){throw "Not fresh: $p"}}
$original=Join-Path $owner 'execution.json'
Checked $original
$proof=Read-Json $original
if(!$proof.passed -or $proof.installed -or $proof.signed -or $proof.job.exit_code -ne 0 -or
   !$proof.job.owned_descendants_settled -or $proof.job.owned_job_final_active_processes -ne 0 -or
   $proof.job.owned_job_forced_termination -or $proof.job.timed_out){throw 'Original stock Job is unavailable'}
$receipt=(Resolve-Path -LiteralPath $proof.stock.path).Path
$run=(Get-Item -LiteralPath $receipt).Directory
if($run.Parent.FullName -ne $packages -or (Split-Path -Leaf $receipt) -ne 'receipt.json' -or
   (Get-FileHash -LiteralPath $receipt).Hash -ne $proof.stock.sha256){throw 'Incorrect stock receipt or parent'}
$stock=Read-Json $receipt
$installer=(Resolve-Path -LiteralPath $proof.installer.path).Path
if($stock.result -ne 'built' -or $stock.signed -or $stock.runtime_build.profile -ne 'debug' -or
   (Split-Path -Parent $installer) -ne (Join-Path $run.FullName 'compiled') -or
   (Get-FileHash -LiteralPath $installer).Hash -ne $stock.installer_sha256 -or $stock.installer_sha256 -ne $proof.installer.sha256){
    throw 'Incorrect final unsigned CPU package'
}
$paths=@($run.FullName,$owner)
$roster=[Collections.Generic.List[object]]::new()
$folders=0
foreach($path in $paths){
    Checked $path
    $queue=[Collections.Generic.Stack[IO.DirectoryInfo]]::new()
    $queue.Push((Get-Item -LiteralPath $path))
    while($queue.Count){
        $folder=$queue.Pop()
        if($folder.Attributes -band [IO.FileAttributes]::ReparsePoint){throw "Redirected directory $folder"}
        $acl=[IO.FileSystemAclExtensions]::GetAccessControl($folder)
        if($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid){throw "Foreign directory $folder"}
        $null=Assert-AirkDirectoryPolicy $acl $human
        $folders++
        foreach($entry in $folder.EnumerateFileSystemInfos()){
            if($entry.Attributes -band [IO.FileAttributes]::ReparsePoint){throw "Redirected output $entry"}
            if($entry -is [IO.DirectoryInfo]){$queue.Push($entry);continue}
            $acl=[IO.FileSystemAclExtensions]::GetAccessControl($entry)
            if($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid){throw "Foreign file $entry"}
            $stream=[IO.File]::Open($entry.FullName,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::None)
            try{$hash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream)).ToLowerInvariant()}
            finally{$stream.Dispose()}
            $roster.Add(@{path=$entry.FullName;sha256=$hash;bytes=$entry.Length})
        }
    }
}
$retired=@()
foreach($name in @('tested-runtime','qualified-runtime')){
    $dir=(Resolve-Path -LiteralPath (Join-Path $run.FullName $name)).Path
    if((Split-Path -Parent $dir) -ne $run.FullName -or (Split-Path -Leaf $dir) -ne $name){throw "Unexpected target $dir"}
    $manifest=Read-Json (Join-Path $dir 'airk-install.json')
    $prefix=$dir+[IO.Path]::DirectorySeparatorChar
    $files=@($roster|Where-Object {$_.path.StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase)})
    if($files.Count -ne 1+$manifest.files.Count){throw 'Incomplete physical package tree'}
    foreach($entry in $files){
        $relative=[IO.Path]::GetRelativePath($dir,$entry.path).Replace('\','/')
        if($relative -ne 'airk-install.json' -and $entry.sha256 -ne $manifest.files[$relative]){throw "Manifest mismatch $relative"}
    }
    $retired+=@{path=$dir;files=$files.Count;bytes=($files|Measure-Object -Sum -Property {$_['bytes']}).Sum}
}
$ledger=Join-Path $campaign 'windows-stock-installer-closed-custody-r1.json'
if(!$Reclaim){
    Fresh $ledger
    [IO.File]::WriteAllText($ledger,(@{original=(Get-FileHash $original).Hash;stock=(Get-FileHash $receipt).Hash;installer=$proof.installer;
      before=$roster;directories=$folders;owner_sid=$sid;transient=$retired}|ConvertTo-Json -Compress -Depth 8))
    Checked $ledger
    (Get-FileHash $ledger).Hash
    return
}
Checked $ledger
$pre=Read-Json $ledger
if($pre.before.Count -ne $roster.Count -or $pre.stock -ne (Get-FileHash $receipt).Hash){throw 'Original roster changed'}
$lookup=@{};foreach($entry in $pre.before){$lookup[$entry.path]=$entry.sha256}
foreach($entry in $roster){if($lookup[$entry.path] -ne $entry.sha256){throw "Changed after custody: $($entry.path)"}}
$post=Join-Path $campaign 'windows-stock-installer-transient-reclaimed-r1.json'
Fresh $post
foreach($entry in $retired){
    $target=(Resolve-Path -LiteralPath $entry.path).Path
    if((Split-Path -Parent $target) -ne $run.FullName -or (Split-Path -Leaf $target) -notin @('tested-runtime','qualified-runtime')){
        throw "Invalid cleanup target $target"
    }
    Remove-Item -LiteralPath $target -Recurse -Force
}
if((Get-FileHash -LiteralPath $installer).Hash -ne $proof.installer.sha256){throw 'Final installer changed'}
$layout=(Resolve-Path -LiteralPath (Join-Path $run.FullName 'layout')).Path
foreach($entry in $roster){
    if($entry.path.StartsWith($layout+'\',[StringComparison]::OrdinalIgnoreCase) -and
       (Get-FileHash -LiteralPath $entry.path).Hash -ne $entry.sha256){throw "Final layout changed: $($entry.path)"}
}
$originalRuntime=(Resolve-Path -LiteralPath (Join-Path $campaign 'windows-fresh-cpu-product-r2/runtime')).Path
$manifest=Join-Path $originalRuntime 'airk-install.json'
if((Get-FileHash -LiteralPath $manifest).Hash -ne '20ea74f79a7416ce268c0de8d29be950aaf4c199df614444692ea37c84564302'){throw 'Original CPU manifest changed'}
foreach($entry in (Read-Json $manifest).files.GetEnumerator()){
    $file=(Resolve-Path -LiteralPath (Join-Path $originalRuntime $entry.Key)).Path
    if(!$file.StartsWith($originalRuntime+'\',[StringComparison]::OrdinalIgnoreCase) -or
       (Get-FileHash -LiteralPath $file).Hash -ne $entry.Value){throw "Original tested CPU runtime changed: $($entry.Key)"}
}
[IO.File]::WriteAllText($post,(@{custody_sha256=(Get-FileHash $ledger).Hash;retired=$retired;retained_installer=$proof.installer;
    retained_layout=(Join-Path $run.FullName 'layout');original_tested=(Join-Path $campaign 'windows-fresh-cpu-product-r2/runtime')}|
    ConvertTo-Json -Compress -Depth 6))
Checked $post
(Get-FileHash $post).Hash
