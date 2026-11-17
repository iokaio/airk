# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$workspace = (Get-Item -LiteralPath 'D:/code/Github/airk').FullName
$parent = Get-Item -LiteralPath (Join-Path $workspace 'artifacts')
if (($parent.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Redirected fixture parent' }
$privateBase = Join-Path $parent.FullName ('ef26-' + [Guid]::NewGuid().ToString('N').Substring(0,8))
if (Test-Path -LiteralPath $privateBase) { throw 'Fresh fixture collision' }
New-Item -ItemType Directory -Path $privateBase -ErrorAction Stop | Out-Null
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
$acl = New-Object Security.AccessControl.DirectorySecurity
$acl.SetOwner($sid)
$acl.SetAccessRuleProtection($true, $false)
$rule = New-Object Security.AccessControl.FileSystemAccessRule($sid, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
$acl.AddAccessRule($rule)
Set-Acl -LiteralPath $privateBase -AclObject $acl
$verified = Get-Acl -LiteralPath $privateBase
if (-not $verified.AreAccessRulesProtected -or $verified.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid.Value) { throw 'Private ownership failed' }
$rules = @($verified.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier]))
if ($rules.Count -ne 1 -or $rules[0].IdentityReference.Value -ne $sid.Value -or $rules[0].IsInherited -or $rules[0].AccessControlType -ne 'Allow' -or $rules[0].FileSystemRights -ne 'FullControl') { throw 'Unexpected private access' }
$receipt = Join-Path $workspace 'artifacts/local/one-airk-1.8.14/completion-editor-fixture-base-r26.json'
if (Test-Path -LiteralPath $receipt) { throw 'Existing base receipt must be retained' }
@{schema=1;path=$privateBase;owner=$sid.Value;sddl=$verified.Sddl;purpose='r26 authentic legacy editor fixture; short protected path'} | ConvertTo-Json | Set-Content -LiteralPath $receipt
python scripts/prepare-native-editor-legacy.py --runtime artifacts/local/one-airk-1.8.14/windows-debug-r26/runtime --previous-runtime artifacts/local/1.8.12-explorer-20261107/runtime --expected-runtime-sha256 2069c2d6e3acc48bba07a808fae4b160c46def14b76b03cb12a768357cf2138a --expected-previous-sha256 be73a42fb700d058dd4b2719a672d82c7a82ad9e0e5eb527529c2f23ac4ae30a --owner (Join-Path $privateBase 'legacy')
exit $LASTEXITCODE
