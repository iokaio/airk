# SPDX-License-Identifier: Apache-2.0
<#
.SYNOPSIS
Runs the indexing plan's measurements (steps P0.3 and P7.2) in a fixed order.

.DESCRIPTION
For a small and a large private source copy it measures the cold base pass,
a rescan with nothing changed and, on the small one, one edited file. With a
model profile it then builds every vector for the small source and samples
searches. Each operation is one receipt written by baseline.py.

The run folder must already hold sources\<name> for each source and the
folders data, receipts, logs and tmp. Everything is written below it, as the
verified human, and each created index is audited for ownership.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Harness,
    [Parameter(Mandatory)][string]$Run,
    [Parameter(Mandatory)][string]$Root,
    [Parameter(Mandatory)][string]$ExpectedSid,
    [Parameter(Mandatory)][string]$Small,
    [string]$Large,
    [string]$ModelDir,
    [string]$EditFile = 'README.md',
    [string]$LexicalQuery = 'error',
    [string]$VectorQuery = 'how are artifacts stored and versioned',
    [ValidateSet('small-base', 'large-base', 'small-vectors')][string[]]$Steps = @('small-base', 'large-base', 'small-vectors')
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '..\dev\assert-human-execution.ps1')
Assert-AirkHumanExecution -ExpectedSid $ExpectedSid | Out-Null
$script = Join-Path $PSScriptRoot 'baseline.py'
$log = Join-Path $Run 'logs\commands.log'
foreach ($required in $Harness, (Join-Path $Run 'data'), (Join-Path $Run 'receipts'), (Join-Path $Run 'logs'), (Join-Path $Run 'tmp')) {
    if (-not (Test-Path -LiteralPath $required)) { throw "Missing input: $required" }
}

function New-StateFolder([string]$Label) {
    $path = Join-Path $Run "data\$Label"
    if (Test-Path -LiteralPath $path) { throw "State folder already exists; measurements need a fresh one: $path" }
    Assert-AirkOutputParent -Path $path -Root $Root -ExpectedSid $ExpectedSid | Out-Null
    New-Item -ItemType Directory -Path $path | Out-Null
    Assert-AirkCreatedOutput -Path $path -Root $Root -ExpectedSid $ExpectedSid | Out-Null
    $path
}

function Invoke-Measurement([string]$Label, [string]$Source, [string]$Data, [string[]]$Operation) {
    $receipt = Join-Path $Run "receipts\$Label.json"
    if (Test-Path -LiteralPath $receipt) { throw "Receipt already exists: $receipt" }
    Assert-AirkOutputParent -Path $receipt -Root $Root -ExpectedSid $ExpectedSid | Out-Null
    $free = (Get-PSDrive ((Resolve-Path $Run).Drive.Name)).Free
    $started = Get-Date
    $arguments = @('-I', '-B', $script, 'run', '--harness', $Harness, '--source', $Source, '--data', $Data,
        '--temp', (Join-Path $Run 'tmp'), '--label', $Label, '--output', $receipt) + $Operation
    & python @arguments
    $code = $LASTEXITCODE
    $elapsed = [math]::Round(((Get-Date) - $started).TotalSeconds, 1)
    $after = (Get-PSDrive ((Resolve-Path $Run).Drive.Name)).Free
    Add-Content -LiteralPath $log -Value ("{0} label={1} operation={2} exit={3} elapsed_s={4} free_before={5} free_after={6}" -f $started.ToUniversalTime().ToString('o'), $Label, ($Operation -join ' '), $code, $elapsed, $free, $after)
    if ($code -ne 0) { throw "Measurement failed: $Label" }
    Assert-AirkCreatedOutput -Path $receipt -Root $Root -ExpectedSid $ExpectedSid | Out-Null
}

# Every file of a created index must belong to the verified human before any
# later operation consumes it.
function Assert-StateOwnership([string]$Label, [string]$Data) {
    $files = 0; $foreign = 0
    foreach ($file in [System.IO.Directory]::EnumerateFileSystemEntries($Data, '*', [System.IO.SearchOption]::AllDirectories)) {
        $files++
        if ((Get-Acl -LiteralPath $file).GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $ExpectedSid) { $foreign++ }
    }
    $audit = Join-Path $Run "receipts\$Label.owner-audit.json"
    [ordered]@{ schema = 'airk.private.owner-audit/1'; label = $Label; entries = $files; foreign = $foreign; expected_sid = $ExpectedSid; observed_utc = (Get-Date).ToUniversalTime().ToString('o') } |
        ConvertTo-Json | Set-Content -LiteralPath $audit -Encoding utf8
    if ($foreign -ne 0) { throw "Index $Label holds $foreign entries with a foreign owner" }
}

$smallSource = Join-Path $Run "sources\$Small"
if ($Steps -contains 'small-base') {
    $data = New-StateFolder 'small-base'
    Invoke-Measurement 'small-base-cold' $smallSource $data @('--operation', 'index')
    Assert-StateOwnership 'small-base' $data
    Invoke-Measurement 'small-base-rescan' $smallSource $data @('--operation', 'index')
    $marker = 'airkbaselinemarker' + [guid]::NewGuid().ToString('N')
    Invoke-Measurement 'small-base-edit' $smallSource $data @('--operation', 'edit', '--edit-file', $EditFile, '--marker', $marker)
}
if ($Steps -contains 'large-base') {
    if (-not $Large) { throw 'The large-base step needs -Large' }
    $largeSource = Join-Path $Run "sources\$Large"
    $data = New-StateFolder 'large-base'
    Invoke-Measurement 'large-base-cold' $largeSource $data @('--operation', 'index')
    Assert-StateOwnership 'large-base' $data
    Invoke-Measurement 'large-base-rescan' $largeSource $data @('--operation', 'index')
}
if ($Steps -contains 'small-vectors') {
    if (-not $ModelDir) { throw 'The small-vectors step needs -ModelDir' }
    $data = New-StateFolder 'small-vectors'
    Invoke-Measurement 'small-vectors-install' $smallSource $data @('--operation', 'install-model', '--model-dir', $ModelDir)
    Invoke-Measurement 'small-vectors-index' $smallSource $data @('--operation', 'index')
    Assert-StateOwnership 'small-vectors' $data
    Invoke-Measurement 'small-vectors-search-lexical' $smallSource $data @('--operation', 'search', '--mode', 'lexical', '--query', $LexicalQuery)
    Invoke-Measurement 'small-vectors-search-vector' $smallSource $data @('--operation', 'search', '--mode', 'vector', '--query', $VectorQuery)
}
"Completed steps: $($Steps -join ', ')"
