# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference='Stop'
$anchor=Join-Path $PSScriptRoot ('qasper-anchor-control-'+[guid]::NewGuid().ToString('N'))
$observer=Join-Path $PSScriptRoot 'observe-qasper-final-anchor-r2.ps1'
New-Item -ItemType Directory -Path $anchor | Out-Null
try {
 $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User
 $acl=Get-Acl -LiteralPath $anchor
 if($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid.Value){throw 'synthetic directory owner differs'}
 # Change DACL only. Set-Acl on this host also requests privileged security sections.
 & icacls.exe $anchor '/inheritance:r' '/grant:r' (('*{0}:(OI)(CI)F' -f $sid.Value)) | Out-Null
 if($LASTEXITCODE -ne 0){throw 'synthetic DACL setup failed'}
 $positive=& $observer -Anchor $anchor | ConvertFrom-Json
 if (-not $positive.current_owner_only) {throw 'positive proof failed'}
 & icacls.exe $anchor '/grant' '*S-1-1-0:RX' | Out-Null
 if($LASTEXITCODE -ne 0){throw 'synthetic foreign DACL setup failed'}
 $refused=$false
 try { & $observer -Anchor $anchor | Out-Null } catch { $refused=$true }
 if(-not $refused){throw 'foreign ACL accepted'}
 $out=Join-Path $PSScriptRoot 'qasper-final-anchor-controls-r2.json'
 if(Test-Path -LiteralPath $out){throw 'fresh receipt required'}
 [ordered]@{schema='airk.private-qasper-anchor-controls/1';positive=$positive;foreign_allow_rule_refused=$refused;candidate_content_accessed=$false} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $out -Encoding utf8
 Write-Output 'Two actual Windows ACL controls passed; no candidate content.'
} finally {Remove-Item -LiteralPath $anchor -Force}
