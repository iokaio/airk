# SPDX-License-Identifier: Apache-2.0
param(
    [Parameter(Mandatory=$true)][string]$MatchedRuntime,
    [Parameter(Mandatory=$true)][string]$RuntimeBuildReceipt,
    [Parameter(Mandatory=$true)][string]$RuntimeBuildReceiptSha256,
    [Parameter(Mandatory=$true)][string]$ActualNativeGroup,
    [Parameter(Mandatory=$true)][string]$ActualFullBuildSourceSha256,
    [Parameter(Mandatory=$true)][string]$FreshCardOutput
)
$ErrorActionPreference='Stop'
$env:PYTHONDONTWRITEBYTECODE='1'
$env:PYTHONNOUSERSITE='1'
& 'C:/Users/tdotn/AppData/Local/Programs/Python/Python313/python.exe' -B "$PSScriptRoot/prepare_focus_card.py" `
    --runtime-dir $MatchedRuntime --runtime-build-receipt $RuntimeBuildReceipt `
    --runtime-build-receipt-sha256 $RuntimeBuildReceiptSha256 --expected-native-group $ActualNativeGroup `
    --expected-build-source-sha256 $ActualFullBuildSourceSha256 --output $FreshCardOutput
if ($LASTEXITCODE) { throw 'Fresh artifact card preparation refused; preserve original diagnostic.' }
# Preparation only. Do not execute launch-argv.json until Root reviews the exact card.
