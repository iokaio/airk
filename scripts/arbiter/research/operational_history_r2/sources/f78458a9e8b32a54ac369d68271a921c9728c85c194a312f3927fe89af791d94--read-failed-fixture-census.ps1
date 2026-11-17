# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$base = $PSScriptRoot
$inputs = Get-Content -LiteralPath (Join-Path $base 'failed-fixture-census-inputs.json') -Raw | ConvertFrom-Json
$allProcesses = @(Get-CimInstance Win32_Process)
$results = @()
foreach ($entry in $inputs) {
    $install = Join-Path $entry.root 'install'
    $images = @()
    $journals = @()
    if ($entry.campaign.StartsWith('adoption-')) {
        $receipt = Get-Content -LiteralPath $entry.receipt -Raw | ConvertFrom-Json
        $case = @($receipt.cases | Where-Object {$_.preserved_owner -ceq $entry.root})
        if ($case.Count -ne 1) { throw 'Missing exact adoption case owner.' }
        $caches = @(Get-ChildItem -LiteralPath (Join-Path $entry.root 'User/globalStorage/iokaio.airk/helpers') -Directory)
        if ($caches.Count -ne 1) { throw 'Unexpected copied cache roster.' }
        $cacheImage = Get-Item -LiteralPath (Join-Path $caches[0].FullName 'airk.exe')
        if (($cacheImage.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Copied image redirected.' }
        $cacheHash = (Get-FileHash -LiteralPath $cacheImage.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($cacheHash -cne $case[0].original_helper_sha256) { throw 'Copied historical fixture image differs.' }
        $images += [pscustomobject]@{path=$cacheImage.FullName;sha256=$cacheHash;runtime=$null;version=$case[0].tag;role='authenticated_historical_fixture_copy'}
    }
    foreach ($runtime in @(Get-ChildItem -LiteralPath (Join-Path $install 'runtimes') -Directory)) {
        if ($runtime.Name -cnotmatch '^[a-f0-9]{64}$') { throw 'Unrecognized runtime directory; census must remain bounded.' }
        $manifestPath = Join-Path $runtime.FullName 'airk-install.json'
        if ((Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $runtime.Name) { throw 'Manifest identity differs.' }
        $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
        $approval = Get-Content -LiteralPath (Join-Path $runtime.FullName 'approval.json') -Raw | ConvertFrom-Json
        if ($approval.manifest_sha256 -cne $runtime.Name -or $approval.version -cne $manifest.version) { throw 'Approval identity differs.' }
        foreach ($file in $manifest.files.PSObject.Properties) {
            if ($file.Name -notmatch '\.exe$') { continue }
            if ($file.Name -match '\.\.|^[\\/]|:') { throw 'Unsafe manifest path.' }
            $path = Join-Path $runtime.FullName $file.Name
            $item = Get-Item -LiteralPath $path
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Image redirected.' }
            $hash = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
            if ($hash -cne $file.Value) { throw 'Image identity differs.' }
            $images += [pscustomobject]@{path=$item.FullName;sha256=$hash;runtime=$runtime.Name;version=$approval.version;role='approved_payload'}
        }
    }
    $launcher = Join-Path $install 'airk-launcher.exe'
    if (Test-Path -LiteralPath $launcher) {
        $record = Get-Content -LiteralPath (Join-Path $install 'launcher.json') -Raw | ConvertFrom-Json
        $hash = (Get-FileHash -LiteralPath $launcher -Algorithm SHA256).Hash.ToLowerInvariant()
        # Field checked against saved receipt and approved payload identity below.
        if (@($images | Where-Object { $_.sha256 -ceq $hash }).Count -eq 0) { throw 'Launcher differs from every approved runtime image.' }
        $images += [pscustomobject]@{path=(Get-Item -LiteralPath $launcher).FullName;sha256=$hash;runtime=$null;version=$null;role='legacy_launcher';receipt=$record}
    }
    foreach ($name in @('selected.json','activation.json','upgrade-journal.json','legacy-maintenance.json')) {
        $path = Join-Path $install $name
        if (Test-Path -LiteralPath $path) {
            $body = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
            $journals += [pscustomobject]@{name=$name;sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant();body=$body}
        }
    }
    $live = @()
    foreach ($image in $images) {
        foreach ($process in @($allProcesses | Where-Object { $_.ExecutablePath -and $_.ExecutablePath.Equals($image.path,[StringComparison]::OrdinalIgnoreCase) })) {
            $owner = Invoke-CimMethod -InputObject $process -MethodName GetOwnerSid
            if ($owner.ReturnValue -ne 0) { throw 'Cannot establish process owner.' }
            $live += [pscustomobject]@{pid=$process.ProcessId;parent_pid=$process.ParentProcessId;created=$process.CreationDate.ToString('o');path=$image.path;sha256=$image.sha256;owner_sid=$owner.Sid;role=$image.role}
        }
    }
    $results += [pscustomobject]@{campaign=$entry.campaign;fixture_root=$entry.root;receipt=$entry.receipt;images=$images;journals=$journals;live=$live;live_count=$live.Count}
}
$result = [pscustomobject]@{schema=1;observed_utc=[DateTime]::UtcNow.ToString('o');scope='exact approved payload images and receipt-bound per-install launcher in six preserved failure fixtures; no process control';fixtures=$results;live_count=(@($results | ForEach-Object {$_.live}).Count)}
$result | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath (Join-Path $base 'failed-fixture-owned-census.json') -Encoding utf8
$results | Select-Object campaign,live_count
