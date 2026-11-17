# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference='Stop'
$base=$PSScriptRoot
$all=@(Get-CimInstance Win32_Process)
$rows=@()
foreach($number in @(16)) {
    $receipt=Get-Content -LiteralPath "$base/windows-debug-pre14-adoption-result-r$number.json" -Raw | ConvertFrom-Json
    $case=@($receipt.cases | Where-Object preserved_owner)[0];$fixture=$case.preserved_owner;$install=Join-Path $fixture 'install'
    $selected=Get-Content -LiteralPath (Join-Path $install 'selected.json') -Raw | ConvertFrom-Json
    $image=Join-Path $install ('runtimes/'+$selected.active+'/airk.exe')
    $sha=(Get-FileHash -LiteralPath $image -Algorithm SHA256).Hash.ToLowerInvariant()
    $manifestPath=Join-Path $install ('runtimes/'+$selected.active+'/airk-install.json')
    if((Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $receipt.runtime_manifest_sha256){throw 'Manifest differs from qualified runtime receipt'}
    $manifest=Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    $expected=$manifest.files.'airk.exe'
    if($sha -cne $expected){throw 'Selected image changed'}
    $journal=Get-Content -LiteralPath (Join-Path $install 'legacy-maintenance.json') -Raw | ConvertFrom-Json
    $cacheJournal=Get-Content -LiteralPath (Join-Path $install 'legacy-cache-retirement.json') -Raw | ConvertFrom-Json
    if(@($case.inspection.caches).Count -ne 1){throw 'Unexpected cache roster'}
    $original=$case.inspection.caches[0].executable -replace '^\\\\\?\\',''
    $paths=@($image,$original)
    $observedCache=$original
    if($cacheJournal.withdrawal_operation){
        if($cacheJournal.withdrawal_operation -cne $journal.operation){throw 'Withdrawal crossed operation'}
        $held=Join-Path ([IO.Path]::GetDirectoryName($original)) ('.airk-withheld-'+$cacheJournal.withdrawal_operation)
        $paths+=@($held)
        if(Test-Path -LiteralPath $original){throw 'Withdrawn original path unexpectedly appeared'}
        $observedCache=$held
    }
    $cacheSha=(Get-FileHash -LiteralPath $observedCache -Algorithm SHA256).Hash.ToLowerInvariant()
    if($cacheSha -cne $case.original_helper_sha256){throw 'Historical fixture image changed'}
    $live=@($all | Where-Object {$process=$_;@($paths | Where-Object {$process.ExecutablePath -and $process.ExecutablePath.Equals($_,[StringComparison]::OrdinalIgnoreCase)}).Count -gt 0} | Select-Object ProcessId,ExecutablePath,CreationDate)
    $rows+=@{case="adoption-r$number";fixture=$fixture;phase=$journal.phase;operation=$journal.operation;images=$paths;image_sha256=@($sha,$cacheSha);live=$live;live_count=$live.Count;cache_retirement_pending=$true;withdrawal_operation=$cacheJournal.withdrawal_operation}
}
$result=@{schema=1;observed_utc=[DateTime]::UtcNow.ToString('o');scope='exact selected plus original/receipt-bound withheld historical cache images';fixtures=$rows}
$result | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath "$base/preserved-adoption-r16-readonly-census.json" -Encoding utf8
$rows | Select-Object case,phase,live_count,withdrawal_operation
