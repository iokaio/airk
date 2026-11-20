# SPDX-License-Identifier: Apache-2.0
# Dot-source this function before invoking original Cargo commands in the Windows campaign.
function Use-AirkOfflineCargo {
    param([Parameter(Mandatory)][string]$Scratch, [switch]$SeedProbe)
    $workspace=(Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
    $campaign=(Resolve-Path (Join-Path $workspace 'artifacts/v190/final-plan/exec-20261116T052942Z')).Path
    $sid='S-1-5-21-3678370870-3858642417-826968105-1002'
    . (Join-Path $workspace 'scripts/dev/assert-human-execution.ps1')
    $null=Assert-AirkHumanExecution $sid
    $shared=(Resolve-Path (Join-Path $campaign 'windows-offline-cargo-r1')).Path
    $scratchPath=(Resolve-Path -LiteralPath $Scratch).Path
    if(!$scratchPath.StartsWith($campaign+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Use an existing private D campaign scratch'}
    $cargoRoot=Join-Path $shared 'home'
    foreach($path in @($shared,$cargoRoot,$scratchPath)){$null=Assert-AirkCreatedOutput $path $campaign $sid}
    if(!$SeedProbe){
        $receipt=Get-Content -LiteralPath (Join-Path $shared 'execution.json') -Raw|ConvertFrom-Json
        if(!$receipt.passed -or $receipt.root -ne $cargoRoot -or $receipt.owner -ne $sid -or
           $receipt.lock_sha256 -ne (Get-FileHash (Join-Path $workspace 'Cargo.lock')).Hash -or
           $receipt.environment_owner -ne (Get-FileHash $PSCommandPath).Hash){throw 'Original verified D Cargo cache required'}
    }
    $toolRoot='C:\Users\tdotn\.rustup\toolchains\1.98.0-x86_64-pc-windows-msvc\bin'
    $env:CARGO_HOME=$cargoRoot
    $env:CARGO_NET_OFFLINE='true'
    $env:CARGO_CACHE_AUTO_CLEAN_FREQUENCY='never'
    $env:RUSTC=Join-Path $toolRoot 'rustc.exe'
    $env:RUSTDOC=Join-Path $toolRoot 'rustdoc.exe'
    $env:CARGO=Join-Path $toolRoot 'cargo.exe'
    $env:TMP=$env:TEMP=$env:TMPDIR=$scratchPath
    $env:PATH="$toolRoot;C:\Users\tdotn\.cargo\bin;"+$env:PATH
    if((Get-Command cargo.exe).Source -ne $env:CARGO){throw 'Pinned native Cargo missing'}
}
