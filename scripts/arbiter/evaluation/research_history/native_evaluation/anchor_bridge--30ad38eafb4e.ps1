# SPDX-License-Identifier: Apache-2.0
param(
    [Parameter(Mandatory=$true)][ValidateSet('Create','Observe')][string]$Mode,
    [Parameter(Mandatory=$true)][string]$Anchor,
    [Parameter(Mandatory=$true)][string]$Producer,
    [Parameter(Mandatory=$true)][string]$Observer
)
$ErrorActionPreference = 'Stop'
if (-not [IO.Path]::IsPathFullyQualified($Anchor)) { throw 'absolute_anchor_required' }
if ($Mode -eq 'Create') {
    . $Producer -DefineOnly
    New-FreshCurrentSidAnchor -Anchor $Anchor -Observer $Observer
} else {
    # Repeat the producer's existing no-reparse ancestry rule at every cut.
    $item = Get-Item -LiteralPath $Anchor -Force
    while ($null -ne $item) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'aliased_anchor_ancestor_refused' }
        $item = $item.Parent
    }
    & $Observer -Anchor $Anchor
}
