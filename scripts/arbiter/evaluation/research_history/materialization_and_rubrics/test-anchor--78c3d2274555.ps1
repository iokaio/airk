# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'prepare-anchor.ps1') -DefineOnly
$control = Join-Path $PSScriptRoot ('opaque-final-acl-control-' + [guid]::NewGuid().ToString('N'))
$observer = Join-Path (Split-Path -Parent $PSScriptRoot) 'observe-qasper-final-anchor-r3.ps1'
$positive = $false
$noPropagateRefused = $false
$foreignRefused = $false
try {
    $proof = New-FreshCurrentSidAnchor -Anchor $control -Observer $observer | ConvertFrom-Json
    if (-not $proof.current_owner_only -or -not $proof.no_reparse -or $proof.current_sid -ne $proof.owner_sid) {
        throw 'actual_synthetic_positive_failed'
    }
    $positive = $true
    $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
    & icacls.exe $control '/remove:g' (('*{0}' -f $sid.Value)) | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'synthetic_replace_failed' }
    & icacls.exe $control '/grant:r' (('*{0}:(OI)(CI)(NP)F' -f $sid.Value)) | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'synthetic_no_propagate_failed' }
    try { & $observer -Anchor $control | Out-Null } catch { $noPropagateRefused = $true }
    if (-not $noPropagateRefused) { throw 'no_propagate_acl_was_accepted' }
    & icacls.exe $control '/grant:r' (('*{0}:(OI)(CI)F' -f $sid.Value)) | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'synthetic_restore_failed' }
    & icacls.exe $control '/grant' '*S-1-1-0:RX' | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'synthetic_foreign_rule_failed' }
    try { & $observer -Anchor $control | Out-Null } catch { $foreignRefused = $true }
    if (-not $foreignRefused) { throw 'foreign_acl_was_accepted' }
    $receipt = Join-Path $PSScriptRoot 'synthetic-acl-after-r2.json'
    if (Test-Path -LiteralPath $receipt) { throw 'fresh_control_receipt_required' }
    [ordered]@{ schema='airk.private-fresh-final-synthetic-acl/1'; selected=3; passed=3;
        actual_current_sid_positive=$positive; no_propagate_refused=$noPropagateRefused;
        foreign_allow_rule_refused=$foreignRefused; real_final_or_gold_accessed=$false;
        no_cross_user_gold_grant=$true; synthetic_empty_foreign_read_rule_only=$true;
        no_owner_change=$true } | ConvertTo-Json | Set-Content -LiteralPath $receipt -Encoding utf8
    Write-Output '{"synthetic_acl_selected":3,"synthetic_acl_passed":3,"real_final_or_gold_accessed":false}'
} finally {
    if (Test-Path -LiteralPath $control) {
        # Only this exact fresh empty control directory; never recurse or touch
        # a production anchor, data or a preexisting path.
        $resolved = (Get-Item -LiteralPath $control -Force).FullName
        $expectedParent = (Get-Item -LiteralPath $PSScriptRoot -Force).FullName
        if ((Split-Path -Parent $resolved) -ne $expectedParent -or
            (Split-Path -Leaf $resolved) -notmatch '^opaque-final-acl-control-[0-9a-f]{32}$') {
            throw 'unsafe_control_cleanup_path'
        }
        Remove-Item -LiteralPath $resolved -Force
    }
}
