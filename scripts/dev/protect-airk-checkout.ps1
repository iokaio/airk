# SPDX-License-Identifier: Apache-2.0
<#
.SYNOPSIS
Preview or apply a private write-permission boundary to one AIRK checkout.

.DESCRIPTION
Run as the checkout owner. The default is a read-only preview. -Apply saves
the original root DACL to a new file in LocalApplicationData before changing
the checkout root. Inheriting children receive the new root permissions.
-Probe lists additional repository-relative paths to inspect, including
children with independent ACLs. The script does not change file ownership.

.EXAMPLE
pwsh -NoProfile -File scripts/dev/protect-airk-checkout.ps1 -Repository D:\code\Github\airk

.EXAMPLE
pwsh -NoProfile -File scripts/dev/protect-airk-checkout.ps1 -Repository D:\code\Github\airk -Apply
#>
[CmdletBinding(SupportsShouldProcess)]
param(
    [Parameter(Mandatory)][string]$Repository,
    [string[]]$Probe = @('scripts', 'artifacts'),
    [switch]$Apply
)

$ErrorActionPreference = 'Stop'
if (!$IsWindows) { throw 'Windows PowerShell 7 is required.' }

$directory = Get-Item -LiteralPath $Repository
$root = $directory.FullName
if (!$directory.PSIsContainer -or $root -eq [IO.Path]::GetPathRoot($root) -or
    !(Test-Path -LiteralPath (Join-Path $root '.git')) -or
    !(Test-Path -LiteralPath (Join-Path $root 'crates/airk-core/Cargo.toml') -PathType Leaf)) {
    throw 'Specify the root of one AIRK Git checkout.'
}
for ($parent = $directory; $null -ne $parent; $parent = $parent.Parent) {
    if ($parent.Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw "Redirected checkout path: $($parent.FullName)"
    }
}

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$user = $identity.User.Value
$acl = Get-Acl -LiteralPath $root
$sidType = [Security.Principal.SecurityIdentifier]
if ($identity.Owner.Value -ne $user -or $acl.GetOwner($sidType).Value -ne $user) {
    throw 'Run as the checkout owner with that account as the token default owner.'
}

$trusted = @($user, 'S-1-5-18', 'S-1-5-32-544',
    'S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464')
# Match the write, delete and ACL/owner authority mask used by CUDA custody.
$writeMask = 0x500d0156
$access = [Security.AccessControl.AccessControlSections]::Access
$inherited = [Security.AccessControl.AceFlags]::Inherited
$allow = [Security.AccessControl.AceQualifier]::AccessAllowed
$original = $acl.GetSecurityDescriptorSddlForm($access)
$raw = [Security.AccessControl.RawSecurityDescriptor]::new($original)
if (!$acl.AreAccessRulesCanonical -or $null -eq $raw.DiscretionaryAcl) {
    throw 'The checkout has an unexpected DACL. Review it manually.'
}

$probes = @(
    foreach ($relative in $Probe) {
        $path = [IO.Path]::GetFullPath((Join-Path $root $relative))
        if (!$path.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase)) {
            throw "Audit path must be inside the checkout: $relative"
        }
        for ($cursor = $path; $cursor -ne $root; $cursor = [IO.Path]::GetDirectoryName($cursor)) {
            if ((Get-Item -LiteralPath $cursor).Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw "Redirected audit path: $cursor"
            }
        }
        $path
    }
)

$changes = @(
    for ($index = $raw.DiscretionaryAcl.Count - 1; $index -ge 0; $index--) {
        $ace = $raw.DiscretionaryAcl[$index]
        if ($ace -isnot [Security.AccessControl.CommonAce] -or $ace.IsCallback) {
            throw 'The checkout contains an uncommon ACL entry. Review it manually.'
        }
        $wasInherited = [bool]($ace.AceFlags -band $inherited)
        $ace.AceFlags = $ace.AceFlags -band (-bnot $inherited)
        if ($ace.AceQualifier -ne $allow -or $ace.SecurityIdentifier.Value -in $trusted -or
            !($ace.AccessMask -band $writeMask)) { continue }
        $before = $ace.AccessMask
        $after = $before -band (-bnot $writeMask)
        [pscustomobject]@{
            SID = $ace.SecurityIdentifier.Value
            Inherited = $wasInherited
            Before = '0x{0:x8}' -f $before
            After = '0x{0:x8}' -f $after
        }
        if ($after -eq 0) {
            $raw.DiscretionaryAcl.RemoveAce($index)
        } else {
            $ace.AccessMask = $after
        }
    }
)

$raw.SetFlags($raw.ControlFlags -bor
    [Security.AccessControl.ControlFlags]::DiscretionaryAclProtected)
$acl.SetSecurityDescriptorSddlForm($raw.GetSddlForm($access), $access)
if (!$acl.AreAccessRulesCanonical -or !$acl.AreAccessRulesProtected) {
    throw 'The proposed DACL fails the safety checks. Review it manually.'
}
"Checkout: $root"
$changes | Format-Table -AutoSize

if ($Apply -and $PSCmdlet.ShouldProcess($root, 'Protect the checkout DACL and remove foreign write grants')) {
    if ((Get-Acl -LiteralPath $root).GetSecurityDescriptorSddlForm($access) -cne $original) {
        throw 'Checkout permissions changed during preview. Start again.'
    }
    $backup = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) (
        'airk-checkout-acl-' + [guid]::NewGuid().ToString('N') + '.sddl'
    )
    [IO.File]::WriteAllText($backup, $original)
    "Original DACL backup: $backup"
    Set-Acl -LiteralPath $root -AclObject $acl
    if (!(Get-Acl -LiteralPath $root).AreAccessRulesProtected) {
        throw "Checkout root did not retain protected inheritance. Backup: $backup"
    }
}

foreach ($path in @($root) + $probes) {
    $actual = Get-Acl -LiteralPath $path
    foreach ($rule in $actual.GetAccessRules($true, $true, $sidType)) {
        if ($rule.AccessControlType -eq 'Allow' -and
            $rule.IdentityReference.Value -notin $trusted -and
            ($rule.FileSystemRights.value__ -band $writeMask)) {
            [pscustomobject]@{
                Path = $path
                SID = $rule.IdentityReference.Value
                Inherited = $rule.IsInherited
                Rights = $rule.FileSystemRights
            }
        }
    }
}
