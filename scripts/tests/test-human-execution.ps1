# SPDX-License-Identifier: Apache-2.0
# Read-only policy tests: ACL fixtures exist only in memory, never on disk.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../dev/assert-human-execution.ps1')
$script:passed = 0
function Pass([string]$Name, [scriptblock]$Action) {
    & $Action | Out-Null
    $script:passed++
    Write-Output "PASS $Name"
}
function Refuse([string]$Name, [scriptblock]$Action) {
    $refused = $false
    try { & $Action | Out-Null } catch { $refused = $true }
    if (-not $refused) { throw "Expected refusal: $Name" }
    $script:passed++
    Write-Output "PASS $Name"
}
$sid = 'S-1-5-21-1-2-3-1002'
$other = 'S-1-5-21-1-2-3-1003'
function Observation {
    @{ UserSid=$sid; OwnerSid=$sid; SessionId=1; InteractiveOwnerSids=@($sid); TokenSids=@($sid,'S-1-1-0') }
}
function Policy([string]$Owner=$sid, [string]$Inheritance='ContainerInherit, ObjectInherit', [string]$Propagation='None', [string]$Rights='FullControl') {
    $acl=[Security.AccessControl.DirectorySecurity]::new()
    $acl.SetOwner([Security.Principal.SecurityIdentifier]::new($Owner))
    $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($sid), $Rights, $Inheritance, $Propagation, 'Allow'))
    return $acl
}
Pass 'matching identity' { Assert-AirkIdentityObservation (Observation) $sid }
Refuse 'effective user mismatch' { $o=Observation; $o.UserSid=$other; Assert-AirkIdentityObservation $o $sid }
Refuse 'default owner mismatch' { $o=Observation; $o.OwnerSid=$other; Assert-AirkIdentityObservation $o $sid }
Refuse 'noninteractive session' { $o=Observation; $o.SessionId=0; Assert-AirkIdentityObservation $o $sid }
Refuse 'absent Explorer' { $o=Observation; $o.InteractiveOwnerSids=@(); Assert-AirkIdentityObservation $o $sid }
Refuse 'different interactive owner' { $o=Observation; $o.InteractiveOwnerSids=@($other); Assert-AirkIdentityObservation $o $sid }
Refuse 'ambiguous interactive owner' { $o=Observation; $o.InteractiveOwnerSids=@($sid,$other); Assert-AirkIdentityObservation $o $sid }
Pass 'same owner multiple Explorer instances' { $o=Observation; $o.InteractiveOwnerSids=@($sid,$sid); Assert-AirkIdentityObservation $o $sid }
Pass 'human parent and inherited full control' { Assert-AirkDirectoryPolicy (Policy) (Observation) }
Refuse 'foreign directory owner' { Assert-AirkDirectoryPolicy (Policy -Owner $other) (Observation) }
Refuse 'file inheritance only' { Assert-AirkDirectoryPolicy (Policy -Inheritance ObjectInherit) (Observation) }
Refuse 'directory inheritance only' { Assert-AirkDirectoryPolicy (Policy -Inheritance ContainerInherit) (Observation) }
Refuse 'no inheritance' { Assert-AirkDirectoryPolicy (Policy -Inheritance None) (Observation) }
Refuse 'inherit only lacks parent access' { Assert-AirkDirectoryPolicy (Policy -Propagation InheritOnly) (Observation) }
Refuse 'no propagate lacks descendant policy' { Assert-AirkDirectoryPolicy (Policy -Propagation NoPropagateInherit) (Observation) }
Refuse 'read-only access' { Assert-AirkDirectoryPolicy (Policy -Rights ReadAndExecute) (Observation) }
Refuse 'matching group deny' {
    $acl=Policy
    $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new('S-1-1-0'),'Write','Deny'))
    Assert-AirkDirectoryPolicy $acl (Observation)
}
Pass 'unrelated SID deny does not deny human' {
    $acl=Policy
    $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($other),'Write','Deny'))
    Assert-AirkDirectoryPolicy $acl (Observation)
}
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
Pass 'real source path under root' { Get-AirkCheckedPath $PSCommandPath $root }
Refuse 'sibling prefix escape' { Get-AirkCheckedPath ($root+'-other\x') $root }
Refuse 'dot-dot escape' { Get-AirkCheckedPath ($root+'\..\x') $root }
Refuse 'relative path' { Get-AirkCheckedPath 'scripts/x' $root }
Refuse 'alternate data stream' { Get-AirkCheckedPath ($root+'\x:stream') $root }
Refuse 'UNC path' { Get-AirkCheckedPath '\\host\share\x' $root }
Refuse 'wildcard path' { Get-AirkCheckedPath ($root+'\*.txt') $root }
Refuse 'trailing dot ambiguity' { Get-AirkCheckedPath ($root+'\x.') $root }
Refuse 'trailing space ambiguity' { Get-AirkCheckedPath ($root+'\x ') $root }
Refuse 'reserved device name' { Get-AirkCheckedPath ($root+'\NUL.txt') $root }
Refuse 'invalid path characters' { Get-AirkCheckedPath ($root+'\x|y') $root }
Refuse 'drive-wide approval' { Get-AirkCheckedPath $root ([IO.Path]::GetPathRoot($root)) }
Refuse 'reparse ancestor' {
    function Get-Item { [pscustomobject]@{Attributes=[IO.FileAttributes]::ReparsePoint} }
    Get-AirkCheckedPath $PSCommandPath $root
}
Refuse 'implicit OWNER RIGHTS deny' {
    $acl=Policy
    $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new('S-1-3-4'),'Write','Deny'))
    Assert-AirkDirectoryPolicy $acl (Observation)
}
$liveSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
Pass 'live created source owner audit' { Assert-AirkCreatedOutput -Path $PSCommandPath -Root $root -ExpectedSid $liveSid }
Refuse 'wrapper absent immediate parent' {
    Assert-AirkOutputParent -Path (Join-Path $root 'missing-guard-parent-fixture/no-file') -Root $root -ExpectedSid $liveSid
}
Refuse 'wrapper existing foreign leaf' {
    function Get-Acl {
        param($LiteralPath, $ErrorAction)
        if ($LiteralPath -eq $PSCommandPath) { return (Policy -Owner $other) }
        Microsoft.PowerShell.Security\Get-Acl -LiteralPath $LiteralPath -ErrorAction Stop
    }
    Assert-AirkOutputParent -Path $PSCommandPath -Root $root -ExpectedSid $liveSid
}
Refuse 'wrapper absent created output' {
    Assert-AirkCreatedOutput -Path (Join-Path $PSScriptRoot 'missing-guard-output-fixture') -Root $root -ExpectedSid $liveSid
}
Refuse 'path access error is terminating' {
    $ErrorActionPreference = 'Continue'
    function Test-Path {
        [CmdletBinding()]
        param($LiteralPath)
        Write-Error 'Simulated access failure'
    }
    Get-AirkCheckedPath $PSCommandPath $root
}
Write-Output "Selected $script:passed; passed $script:passed; failed 0. No filesystem mutations."
