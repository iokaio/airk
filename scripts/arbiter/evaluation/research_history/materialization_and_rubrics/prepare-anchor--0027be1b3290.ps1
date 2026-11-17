# SPDX-License-Identifier: Apache-2.0
param([switch]$DefineOnly)
# DACL-only producer. Never request SetOwner/privileged security sections.
$ErrorActionPreference = 'Stop'
function New-FreshCurrentSidAnchor {
    param([Parameter(Mandatory=$true)][string]$Anchor,
          [Parameter(Mandatory=$true)][string]$Observer)
    if (Test-Path -LiteralPath $Anchor) { throw 'fresh_anchor_required' }
    $parent = Get-Item -LiteralPath (Split-Path -Parent $Anchor) -Force
    while ($null -ne $parent) {
        if ($parent.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'aliased_parent_refused' }
        $parent = $parent.Parent
    }
    $null = New-Item -ItemType Directory -Path $Anchor
    $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
    $acl = Get-Acl -LiteralPath $Anchor
    if ($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid.Value) {
        throw 'fresh_directory_not_already_current_owner'
    }
    # Existing actual SID1012 controls establish this DACL-only operation.
    # Removing inherited access on a fresh owned directory grants no other UID.
    & icacls.exe $Anchor '/inheritance:r' '/grant:r' (('*{0}:(OI)(CI)F' -f $sid.Value)) | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'current_sid_dacl_setup_failed' }
    & $Observer -Anchor $Anchor
}
if (-not $DefineOnly) {
    $repo = (Get-Location).Path
    $anchor = Join-Path $repo 'artifacts/advanced-split/models/hotpot-qasper-fresh-final-opaque-anchor-r2'
    $observer = Join-Path $repo 'artifacts/advanced-split/models/observe-qasper-final-anchor-r3.ps1'
    $proof = New-FreshCurrentSidAnchor -Anchor $anchor -Observer $observer
    $bytes = [Text.Encoding]::UTF8.GetBytes([string]$proof)
    [ordered]@{ created = $true; proof_sha256 = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant() } | ConvertTo-Json -Compress
}
