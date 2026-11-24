# SPDX-License-Identifier: Apache-2.0
<#
.SYNOPSIS
Stage a private fixture image folder for the Answer evaluation (GPU_FIXTURE.md, "Images").

.DESCRIPTION
Copies the service test image and the CLI of one finished build, and the CUDA runtime payload, into a
fresh folder <Base>\answer-eval-gpu-r1\<Prefix>-<first 8 of the service image SHA-256>, and writes
images.json beside them. Set -ProviderStage when the approved CUDA provider is on a different
volume from the image.
Evaluation tooling, never product. Run as the verified human.

Every file is a copy. The runtime refuses a private GPU payload with more than one link
("redirected/nonregular/hardlinked GPU payload"), and a hard link made here would also make the provider
stage's own files multi-link, so every diagnostics image beside them would stop opening (found on
23 November 2026).

.EXAMPLE
pwsh -NoProfile -File stage_fixture_image.ps1 -Prefix lg-gate -ImageLog <image build log> -CliLog <cli build log> `
  -Target <CARGO_TARGET_DIR> -GitHead <commit> -Note "gate image, loop/l5c on <main commit>"
#>
param(
    [Parameter(Mandatory)][string]$Prefix,
    # The log of: cargo test -p airk-service -p airk-kernel --lib --all-features --no-run --message-format=json-render-diagnostics
    [Parameter(Mandatory)][string]$ImageLog,
    [Parameter(Mandatory)][string]$Target,
    [Parameter(Mandatory)][string]$GitHead,
    [Parameter(Mandatory)][string]$Note,
    [string]$CliLog = '',
    [string]$Base = 'C:\Users\tdotn\AppData\Local\airk-final-exec-20261116T052942Z',
    [string]$ExpectedSid = 'S-1-5-21-3678370870-3858642417-826968105-1002',
    [string]$ProviderStage = ''
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '..\dev\assert-human-execution.ps1')
Assert-AirkHumanExecution -ExpectedSid $ExpectedSid | Out-Null
$match = Select-String -Path $ImageLog -Pattern '"executable":"([^"]*airk_service-[0-9a-f]+\.exe)"' | Select-Object -Last 1
if (-not $match) { throw 'no service executable in the build log' }
$built = $match.Matches[0].Groups[1].Value -replace '\\\\', '\'
$sha = (Get-FileHash $built -Algorithm SHA256).Hash.ToLower()
$label = "$Prefix-$($sha.Substring(0, 8))"
$dir = "$Base\answer-eval-gpu-r1\$label"
if (Test-Path $dir) { throw "$dir exists" }
Assert-AirkOutputParent -Path $dir -Root $Base -ExpectedSid $ExpectedSid | Out-Null
$stage = if ($ProviderStage) { $ProviderStage } else { Join-Path $Base 'cu15\gpu-provider-stage' }
$payload = @('airk-cuda-runtime.json', 'fresh-compile.json', 'cublas64_12.dll', 'cublaslt64_12.dll', 'cudart64_12.dll',
    'cudnn_adv64_9.dll', 'cudnn_cnn64_9.dll', 'cudnn_engines_precompiled64_9.dll', 'cudnn_engines_runtime_compiled64_9.dll',
    'cudnn_graph64_9.dll', 'cudnn_heuristic64_9.dll', 'cudnn_ops64_9.dll', 'cudnn64_9.dll',
    'onnxruntime_providers_cuda.dll', 'onnxruntime_providers_shared.dll')
foreach ($name in $payload) {
    $links = @(fsutil hardlink list "$stage\$name")
    if ($LASTEXITCODE -ne 0 -or $links.Count -ne 1) { throw "$stage\$name must be a regular single-link file" }
}
New-Item -ItemType Directory $dir | Out-Null
Assert-AirkCreatedOutput -Path $dir -Root $Base -ExpectedSid $ExpectedSid | Out-Null
foreach ($name in @('airk-service-fixture.exe', 'airkctl.exe', 'images.json') + $payload) {
    Assert-AirkOutputParent -Path "$dir\$name" -Root $Base -ExpectedSid $ExpectedSid | Out-Null
}
Copy-Item -LiteralPath $built -Destination "$dir\airk-service-fixture.exe"
Copy-Item -LiteralPath "$Target\debug\airkctl.exe" -Destination "$dir\airkctl.exe"
foreach ($name in $payload) {
    Copy-Item -LiteralPath "$stage\$name" -Destination "$dir\$name"
}
foreach ($name in @('airk-service-fixture.exe', 'airkctl.exe') + $payload) {
    Assert-AirkCreatedOutput -Path "$dir\$name" -Root $Base -ExpectedSid $ExpectedSid | Out-Null
}
$images = [ordered]@{
    schema = 'airk.private.answer-eval-fixture-images/1'; label = $label
    written_utc = (Get-Date).ToUniversalTime().ToString('o'); git_head = $GitHead; note = $Note
    toolchain = ((& "$env:USERPROFILE\.cargo\bin\rustc.exe" --version) -split ' ')[0..1] -join ' '
    target_directory = $Target
    service = [ordered]@{
        image = [ordered]@{ path = "$dir\airk-service-fixture.exe"; sha256 = $sha; bytes = (Get-Item "$dir\airk-service-fixture.exe").Length }
        built_from = $built
        cargo = 'cargo test -p airk-service -p airk-kernel --lib --all-features --locked --offline --timings --no-run --message-format=json-render-diagnostics'
        log = $ImageLog
    }
    cli = [ordered]@{
        image = [ordered]@{ path = "$dir\airkctl.exe"; sha256 = (Get-FileHash "$dir\airkctl.exe" -Algorithm SHA256).Hash.ToLower() }
        built_from = "$Target\debug\airkctl.exe"
        cargo = 'cargo build -p airkctl --bin airkctl --locked --offline --timings'; log = $CliLog
    }
    cuda_runtime = [ordered]@{
        manifest_sha256 = (Get-FileHash "$dir\airk-cuda-runtime.json" -Algorithm SHA256).Hash.ToLower()
        copied_from = $stage; copied_files = $payload
    }
}
$images | ConvertTo-Json -Depth 6 | Set-Content "$dir\images.json" -Encoding utf8
Assert-AirkCreatedOutput -Path "$dir\images.json" -Root $Base -ExpectedSid $ExpectedSid | Out-Null
"staged $dir service=$sha cli=$($images.cli.image.sha256)"
