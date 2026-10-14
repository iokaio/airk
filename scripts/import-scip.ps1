# SPDX-License-Identifier: Apache-2.0
#Requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Airk,
    [Parameter(Mandatory)][string]$Root,
    [Parameter(Mandatory)][string]$DataDir,
    [string]$Collection = 'default',
    [Parameter(Mandatory)][string]$Producer,
    [Parameter(Mandatory)][string]$ProducerVersion,
    [Parameter(Mandatory)][string]$ProducerExecutable,
    [string[]]$ProducerArguments = @(),
    [Parameter(Mandatory)][string[]]$BuildInput,
    [Parameter(Mandatory)][string]$IndexPath
)
$ErrorActionPreference = 'Stop'
$airkSource = (Resolve-Path -LiteralPath $Root).Path
$airkProgram = (Get-Command $Airk -ErrorAction Stop).Source
$airkProducer = (Get-Command $ProducerExecutable -ErrorAction Stop).Source
$airkOutput = [IO.Path]::GetFullPath($IndexPath)
$airkData = [IO.Path]::GetFullPath($DataDir)
$airkPrefix = $airkSource.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
$airkDataPrefix = $airkData.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
if ($airkOutput.StartsWith($airkPrefix, [StringComparison]::OrdinalIgnoreCase) -and -not $airkOutput.StartsWith($airkDataPrefix, [StringComparison]::OrdinalIgnoreCase)) { throw 'Put producer output outside the source root or inside the excluded AIRK data directory.' }
if (Test-Path -LiteralPath $airkOutput) { throw 'Choose a new index output path; existing output will not be overwritten.' }
$airkManifest = $airkOutput + '.binding.json'
if (Test-Path -LiteralPath $airkManifest) { throw 'Binding path already exists.' }
$airkScope = @('--root', $airkSource, '--data-dir', $airkData, '--collection', $Collection)
& $airkProgram @airkScope index
if ($LASTEXITCODE -ne 0) { throw 'AIRK indexing failed.' }
$airkPrepare = @('analysis', 'prepare', '--producer', $Producer, '--producer-version', $ProducerVersion)
foreach ($airkInput in $BuildInput) { $airkPrepare += @('--build-input', $airkInput) }
$airkBinding = (& $airkProgram @airkScope @airkPrepare | Out-String) | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'AIRK binding preparation failed.' }
Push-Location $airkSource
try {
    # Invoke an explicitly selected executable directly, never a shell command string.
    & $airkProducer @ProducerArguments
    if ($LASTEXITCODE -ne 0) { throw 'SCIP producer failed; prior collection retained.' }
} finally { Pop-Location }
if (-not (Test-Path -LiteralPath $airkOutput -PathType Leaf)) { throw 'Producer did not create the specified SCIP file.' }
$airkBinding.index_sha256 = (Get-FileHash -LiteralPath $airkOutput -Algorithm SHA256).Hash.ToLowerInvariant()
$airkBinding | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $airkManifest -Encoding utf8NoBOM
& $airkProgram @airkScope analysis import $airkOutput --manifest $airkManifest
if ($LASTEXITCODE -ne 0) { throw 'SCIP import rejected; prior collection retained.' }
