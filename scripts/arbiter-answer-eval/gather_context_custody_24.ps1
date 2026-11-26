# SPDX-License-Identifier: Apache-2.0
<#
.SYNOPSIS
Extend checked Gather Context custody to the extra replies in a 24-call diagnostic.

.DESCRIPTION
The original custodian owns all phases and audits the actor working tree.
Replies 013 through 024 occupy reserved leaves in that tree. This companion
checks that the leaves are fresh before launch and only the fixed roster can
remain there after custody. The legacy twelve top-level replies stay fixed.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('Freeze', 'Frozen', 'Prepare', 'Before', 'After', 'Summarize', 'Summarized')]
    [string]$Action,
    [Parameter(Mandatory)][string]$Campaign,
    [Parameter(Mandatory)][string]$Directory,
    [Parameter(Mandatory)][string]$IsolatedHome,
    [Parameter(Mandatory)][string]$ExpectedSid
)

$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'gather_context_custody.ps1') @PSBoundParameters
if ($Action -notin @('Before', 'After')) {
    return
}

. (Join-Path $PSScriptRoot '..\dev\assert-human-execution.ps1')
$identity = Assert-AirkHumanExecution -ExpectedSid $ExpectedSid
$actorPath = Get-AirkCheckedPath -Path $Directory -Root $Campaign
$workPath = Get-AirkCheckedPath -Path (Join-Path $actorPath 'work') -Root $actorPath
$extraNames = @(13..24 | ForEach-Object { 'reply-{0:D3}.json' -f $_ })
if ($Action -eq 'Before') {
    foreach ($name in $extraNames) {
        $path = Join-Path $workPath $name
        $null = Assert-AirkOutputPathObservation -Path $path -Root $Campaign -Identity $identity
        if (Test-Path -LiteralPath $path) {
            throw "Expected fresh extended reply path: $path"
        }
    }
} else {
    foreach ($entry in Get-ChildItem -LiteralPath $workPath -Force) {
        if ($entry.PSIsContainer -or $entry.Name -cnotin $extraNames) {
            throw "Unexpected extended reply output: $($entry.FullName)"
        }
    }
}
