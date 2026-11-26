# SPDX-License-Identifier: Apache-2.0
<#
.SYNOPSIS
Prepare and audit private output custody for a Gather Context diagnostic.

.DESCRIPTION
Prepare creates a fresh actor directory and its four known child directories.
Before checks the isolated Codex home, expected receipt paths and empty actor
directories. After audits the isolated home and all observed actor outputs.
Freeze and Frozen check custody for a diagnostic specification under the
precreated run directory. The checked Windows guard provides read-only
metadata observations. The controller remains responsible for writer
settlement, cache containment, disk reservation and receipt contents.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('Freeze', 'Frozen', 'Prepare', 'Before', 'After', 'Summarize', 'Summarized')][string]$Action,
    [Parameter(Mandatory)][string]$Campaign,
    [Parameter(Mandatory)][string]$Directory,
    [Parameter(Mandatory)][string]$IsolatedHome,
    [Parameter(Mandatory)][string]$ExpectedSid
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '..\dev\assert-human-execution.ps1')
$identity = Assert-AirkHumanExecution -ExpectedSid $ExpectedSid
$campaignPath = Get-AirkCheckedPath -Path $Campaign -Root $Campaign
$actorPath = Get-AirkCheckedPath -Path $Directory -Root $campaignPath
$codexPath = Get-AirkCheckedPath -Path $IsolatedHome -Root $campaignPath
$childNames = @('work', 'codex-logs', 'codex-state', 'codex-tmp')
$receiptNames = @(
    'instruction.txt', 'base-instructions.txt', 'tool-offer.json', 'stream.jsonl',
    'stderr.txt', 'brief.md', 'turns.jsonl', 'session.json', 'judge.json', 'packet.json'
) + @(1..12 | ForEach-Object { 'reply-{0:D3}.json' -f $_ })

if ($actorPath -eq $campaignPath -or $codexPath -eq $campaignPath -or $actorPath -eq $codexPath -or
    $actorPath.StartsWith($codexPath + '\', [StringComparison]::OrdinalIgnoreCase) -or
    $codexPath.StartsWith($actorPath + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Campaign, output and isolated Codex home paths must be distinct and the latter two disjoint'
}

function Assert-CampaignDisk {
    $drive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($campaignPath))
    if ($drive.Name -ne 'D:\' -or $drive.DriveType -ne [IO.DriveType]::Fixed -or -not $drive.IsReady) {
        throw 'Gather Context Windows writes require the approved fixed D: campaign volume'
    }
    # Active Windows Answer-loop campaign floor, docs/agent-current-work.md.
    $available = $drive.AvailableFreeSpace
    if ($available -le 200GB) {
        throw "Campaign free space is below its 200 GiB floor ($available bytes available)"
    }
}

function Assert-CustodyDirectory {
    param([Parameter(Mandatory)][string]$Path)

    Assert-AirkCurrentToken -Identity $identity
    $dir = Get-Item -LiteralPath $Path -Force
    if (-not $dir.PSIsContainer -or ($dir.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "Expected an ordinary custody directory: $Path"
    }
    Assert-AirkDirectoryPolicy -Acl (Get-Acl -LiteralPath $Path) -Identity $identity
}

function Assert-CustodyFile {
    param([Parameter(Mandatory)]$Item)

    Assert-AirkCurrentToken -Identity $identity
    $path = Get-AirkCheckedPath -Path $Item.FullName -Root $campaignPath
    if ($Item.PSIsContainer -or ($Item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
        $Item.LinkType -eq 'HardLink') {
        throw "Expected an ordinary single-location custody file: $path"
    }
    $acl = Get-Acl -LiteralPath $path
    if ($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $ExpectedSid) {
        throw "Custody file is foreign-owned: $path"
    }
    foreach ($rule in $acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier])) {
        $sid = $rule.IdentityReference.Value
        if ($rule.AccessControlType -eq 'Deny' -and ($sid -in $identity.TokenSids -or $sid -eq 'S-1-3-4')) {
            throw "Custody file has an applicable access deny: $path"
        }
    }
}

function Assert-CustodyTree {
    param([Parameter(Mandatory)][string]$Path)

    $pending = [Collections.Generic.Stack[string]]::new()
    $pending.Push($Path)
    while ($pending.Count) {
        $current = $pending.Pop()
        $null = Get-AirkCheckedPath -Path $current -Root $campaignPath
        Assert-CustodyDirectory -Path $current
        foreach ($entry in Get-ChildItem -LiteralPath $current -Force) {
            if ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw "Reparse point in custody tree: $($entry.FullName)"
            }
            if ($entry.PSIsContainer) {
                $pending.Push($entry.FullName)
            } else {
                Assert-CustodyFile -Item $entry
            }
        }
    }
}

function Assert-FreshFilePath {
    param([Parameter(Mandatory)][string]$Path)

    $null = Assert-AirkOutputPathObservation -Path $Path -Root $campaignPath -Identity $identity
    if (Test-Path -LiteralPath $Path) {
        throw "Expected fresh output path: $Path"
    }
}

function New-CustodyDirectory {
    param([Parameter(Mandatory)][string]$Path)

    $null = Assert-AirkOutputParent -Path $Path -Root $campaignPath -ExpectedSid $ExpectedSid
    if (Test-Path -LiteralPath $Path) {
        throw "Actor output directory already exists: $Path"
    }
    $null = [IO.Directory]::CreateDirectory($Path)
    $null = Assert-AirkCreatedOutput -Path $Path -Root $campaignPath -ExpectedSid $ExpectedSid
    Assert-CustodyDirectory -Path $Path
}

Assert-CampaignDisk
$null = Assert-AirkOutputPathObservation -Path $codexPath -Root $campaignPath -Identity $identity
Assert-CustodyDirectory -Path $codexPath
$runPath = [IO.Path]::GetDirectoryName($actorPath)
if ($Action -in @('Prepare', 'Before', 'After') -and $runPath -eq $campaignPath) {
    throw 'Actor directory requires a prepared run directory within the campaign'
}
Assert-CustodyDirectory -Path $runPath

switch ($Action) {
    'Freeze' {
        Assert-CustodyDirectory -Path $actorPath
        $specPath = Join-Path $actorPath 'diagnostic-spec.json'
        Assert-FreshFilePath -Path $specPath
        Assert-FreshFilePath -Path ($specPath + '.tmp')
        if (@(Get-ChildItem -LiteralPath $actorPath -Force).Count) {
            throw 'Expected a fresh diagnostic run directory'
        }
    }
    'Frozen' {
        $specPath = Join-Path $actorPath 'diagnostic-spec.json'
        $null = Assert-AirkCreatedOutput -Path $specPath -Root $campaignPath -ExpectedSid $ExpectedSid
        Assert-CustodyFile -Item (Get-Item -LiteralPath $specPath -Force)
    }
    'Summarize' {
        Assert-CustodyDirectory -Path $actorPath
        Assert-FreshFilePath -Path (Join-Path $actorPath 'diagnostic-results.json')
    }
    'Summarized' {
        $summary = Join-Path $actorPath 'diagnostic-results.json'
        $null = Assert-AirkCreatedOutput -Path $summary -Root $campaignPath -ExpectedSid $ExpectedSid
        Assert-CustodyFile -Item (Get-Item -LiteralPath $summary -Force)
    }
    'Prepare' {
        New-CustodyDirectory -Path $actorPath
        foreach ($name in $childNames) {
            New-CustodyDirectory -Path (Join-Path $actorPath $name)
        }
    }
    'Before' {
        Assert-CustodyDirectory -Path $actorPath
        $entries = @(Get-ChildItem -LiteralPath $actorPath -Force | ForEach-Object Name)
        if ($entries.Count -ne $childNames.Count -or @($entries | Where-Object { $_ -cnotin $childNames }).Count) {
            throw 'Expected exactly the four fresh actor child directories'
        }
        foreach ($name in $childNames) {
            $path = Join-Path $actorPath $name
            Assert-CustodyDirectory -Path $path
            if (@(Get-ChildItem -LiteralPath $path -Force).Count) {
                throw "Expected an empty actor child directory: $path"
            }
        }
        foreach ($name in $receiptNames) {
            $path = Join-Path $actorPath $name
            Assert-FreshFilePath -Path $path
            Assert-FreshFilePath -Path ($path + '.tmp')
        }
        Assert-CustodyTree -Path $codexPath
    }
    'After' {
        Assert-CustodyDirectory -Path $actorPath
        foreach ($name in $childNames) {
            Assert-CustodyTree -Path (Join-Path $actorPath $name)
        }
        foreach ($entry in Get-ChildItem -LiteralPath $actorPath -Force) {
            if ($entry.Name -cin $childNames) {
                continue
            }
            if ($entry.Name -cnotin $receiptNames) {
                throw "Unexpected actor output: $($entry.FullName)"
            }
            Assert-CustodyFile -Item $entry
        }
        Assert-CustodyTree -Path $codexPath
    }
}
