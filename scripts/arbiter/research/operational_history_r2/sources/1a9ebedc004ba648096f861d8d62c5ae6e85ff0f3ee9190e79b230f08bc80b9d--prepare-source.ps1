# SPDX-License-Identifier: Apache-2.0
param([Parameter(Mandatory=$true)][string]$ExpectedHead)
$ErrorActionPreference='Stop'
$workspaceRoot=[System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../..'))
$sourceRoot=Join-Path $PSScriptRoot 'source'
$identity=[System.Security.Principal.WindowsIdentity]::GetCurrent()
$interactive=Get-Process explorer -IncludeUserName|Where-Object{$_.SessionId -eq (Get-Process -Id $PID).SessionId}|Select-Object -First 1
if(-not $interactive -or $interactive.UserName -ne $identity.Name){throw 'Current identity does not match logged-in human'}
if((Get-Acl -LiteralPath $sourceRoot).Owner -ne $identity.Name){throw 'Snapshot source root is not human-owned'}
if(@(Get-ChildItem -LiteralPath $sourceRoot -Force).Count){throw 'Snapshot source destination must be fresh and empty'}
$head=(git -C $workspaceRoot rev-parse HEAD).Trim()
if($head -ne $ExpectedHead){throw 'Source HEAD differs from reviewed freeze'}
$listed=git -C $workspaceRoot ls-files --cached --others --exclude-standard -z
if($LASTEXITCODE -ne 0){throw 'Public source enumeration failed'}
$paths=($listed -join "`n").Split([char]0,[System.StringSplitOptions]::RemoveEmptyEntries)|Sort-Object -Unique
$rows=[System.Collections.Generic.List[object]]::new()
foreach($relative in $paths){
 if($relative -match '^(artifacts|temp|\.airk|\.git|\.codex|\.agents|\.aws)/' -or $relative -eq '.mcp.json'){continue}
 if($relative -match '(^|/)(node_modules|out|dist|target|\.cache)/'){continue}
 if($relative -match '(^|/)\.env($|\.)|(^|/)\.npmrc$'){continue}
 if($relative -match '\.(exe|dll|dylib|so|a|lib)$'){throw "Native binary must not enter UI source snapshot: $relative"}
 if([System.IO.Path]::IsPathRooted($relative) -or $relative.Split('/') -contains '..'){throw 'Invalid source-relative path'}
 $sourceFile=[System.IO.Path]::GetFullPath((Join-Path $workspaceRoot $relative))
 $snapshotFile=[System.IO.Path]::GetFullPath((Join-Path $sourceRoot $relative))
 if(-not $snapshotFile.StartsWith($sourceRoot+'\',[System.StringComparison]::OrdinalIgnoreCase)){throw 'Snapshot output escaped destination'}
 $item=Get-Item -LiteralPath $sourceFile -Force
 if($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint){throw "Source reparse point refused: $relative"}
 $raw=[System.IO.File]::ReadAllBytes($sourceFile)
 $hash=[Convert]::ToHexString([System.Security.Cryptography.SHA256]::HashData($raw)).ToLowerInvariant()
 $parent=Split-Path -Parent $snapshotFile
 [System.IO.Directory]::CreateDirectory($parent)|Out-Null
 if((Get-Acl -LiteralPath $parent).Owner -ne $identity.Name){throw 'Snapshot output parent is not human-owned'}
 $stream=[System.IO.File]::Open($snapshotFile,[System.IO.FileMode]::CreateNew,[System.IO.FileAccess]::Write,[System.IO.FileShare]::None)
 try{$stream.Write($raw,0,$raw.Length)}finally{$stream.Dispose()}
 $owner=(Get-Acl -LiteralPath $snapshotFile).Owner
 if($owner -ne $identity.Name){throw 'Snapshot output file is not human-owned'}
 if((Get-FileHash -LiteralPath $sourceFile -Algorithm SHA256).Hash.ToLowerInvariant() -ne $hash){throw 'Source changed during copy; retain this incomplete snapshot and freeze a new one'}
 $rows.Add([pscustomobject]@{path=$relative;sha256=$hash;bytes=$raw.Length;owner=$owner})
}
foreach($row in $rows){if((Get-FileHash -LiteralPath (Join-Path $workspaceRoot $row.path) -Algorithm SHA256).Hash.ToLowerInvariant() -ne $row.sha256){throw 'Source changed after copy; preserve incomplete snapshot'}}
[pscustomobject]@{schema='airk.ui-source-snapshot/1';head=$head;account=$identity.Name;sid=$identity.User.Value;created_utc=(Get-Date -AsUTC -Format o);files=$rows}|ConvertTo-Json -Depth 5|Set-Content -LiteralPath (Join-Path $PSScriptRoot 'receipts/source-manifest.json') -Encoding utf8
[pscustomobject]@{files=$rows.Count;source=$sourceRoot;head=$head}|ConvertTo-Json
