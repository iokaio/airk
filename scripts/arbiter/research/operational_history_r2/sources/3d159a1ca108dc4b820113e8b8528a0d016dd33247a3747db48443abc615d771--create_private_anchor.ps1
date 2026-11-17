# SPDX-License-Identifier: Apache-2.0
param(
    [Parameter(Mandatory=$true)][string]$ExpectedOwnerSid,
    [string]$Bindings = (Join-Path $PSScriptRoot 'current-bindings.actual.json')
)
$ErrorActionPreference = 'Stop'
$config = Get-Content -LiteralPath $Bindings -Raw -Encoding utf8 | ConvertFrom-Json
$anchor = [IO.Path]::GetFullPath($config.future_paths.private_anchor)
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$allowed = [IO.Path]::GetFullPath((Join-Path $repo 'artifacts/advanced-split/models')) + [IO.Path]::DirectorySeparatorChar
if (-not $anchor.StartsWith($allowed, [StringComparison]::OrdinalIgnoreCase)) { throw 'Private anchor leaves named workspace artifacts parent' }
if (Test-Path -LiteralPath $anchor) { throw 'Fresh private anchor required; preserve existing state' }
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
if ($identity.User.Value -ne $ExpectedOwnerSid) { throw 'Execution account differs from Root-reviewed private owner' }
$directory = [IO.Directory]::CreateDirectory($anchor)
$acl = [Security.AccessControl.DirectorySecurity]::new()
$acl.SetOwner($identity.User)
$acl.SetAccessRuleProtection($true, $false)
$rule = [Security.AccessControl.FileSystemAccessRule]::new(
    $identity.User, [Security.AccessControl.FileSystemRights]::FullControl,
    [Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit',
    [Security.AccessControl.PropagationFlags]::None, [Security.AccessControl.AccessControlType]::Allow)
$acl.AddAccessRule($rule)
[IO.FileSystemAclExtensions]::SetAccessControl($directory, $acl)
$actual = [IO.FileSystemAclExtensions]::GetAccessControl($directory)
$rules = @($actual.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier]))
if (-not $actual.AreAccessRulesProtected -or $actual.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $ExpectedOwnerSid -or $rules.Count -ne 1 -or $rules[0].IdentityReference.Value -ne $ExpectedOwnerSid -or $rules[0].IsInherited -or $rules[0].AccessControlType -ne 'Allow' -or $rules[0].FileSystemRights -ne 'FullControl') { throw 'Exact private owner/ACL proof failed' }
[IO.Directory]::CreateDirectory((Join-Path $anchor 'logs')) | Out-Null
$proof = [ordered]@{ schema='airk.private.composed-anchor-protection/1'; path=$anchor; owner_sid=$ExpectedOwnerSid;
    inheritance_protected=$actual.AreAccessRulesProtected; sddl=$actual.GetSecurityDescriptorSddlForm('All');
    scope='Fresh private source/index/control/fixture state only. No model, native process or source mutation.' }
$proof | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $anchor 'protection.private.json') -Encoding utf8NoBOM
$proof | ConvertTo-Json -Depth 8
