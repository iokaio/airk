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
# Literal split ACE fixtures preserve their distinct flags rather than deriving
# expected grants from the guard's decision logic. Descriptors stay in memory.
function SplitPolicy([string]$DirectoryFlags='CI', [string]$FileFlags='OIIO') {
    # DirectorySecurity canonicalizes equivalent adjacent grants into one ACE.
    # Preserve the literal observed rule list here to exercise separate grants.
    $descriptor=[Security.AccessControl.RawSecurityDescriptor]::new("O:${sid}D:(A;${DirectoryFlags};FA;;;${sid})(A;${FileFlags};FA;;;${sid})")
    $rules=@(foreach ($ace in $descriptor.DiscretionaryAcl) {
        $inheritance=[Security.AccessControl.InheritanceFlags]::None
        if (($ace.AceFlags -band [Security.AccessControl.AceFlags]::ContainerInherit) -ne 0) { $inheritance=$inheritance -bor [Security.AccessControl.InheritanceFlags]::ContainerInherit }
        if (($ace.AceFlags -band [Security.AccessControl.AceFlags]::ObjectInherit) -ne 0) { $inheritance=$inheritance -bor [Security.AccessControl.InheritanceFlags]::ObjectInherit }
        $propagation=[Security.AccessControl.PropagationFlags]::None
        if (($ace.AceFlags -band [Security.AccessControl.AceFlags]::InheritOnly) -ne 0) { $propagation=$propagation -bor [Security.AccessControl.PropagationFlags]::InheritOnly }
        if (($ace.AceFlags -band [Security.AccessControl.AceFlags]::NoPropagateInherit) -ne 0) { $propagation=$propagation -bor [Security.AccessControl.PropagationFlags]::NoPropagateInherit }
        [Security.AccessControl.FileSystemAccessRule]::new($ace.SecurityIdentifier,[Security.AccessControl.FileSystemRights]$ace.AccessMask,$inheritance,$propagation,'Allow')
    })
    $acl=[pscustomobject]@{Owner=$descriptor.Owner;Rules=$rules}
    $acl | Add-Member ScriptMethod GetOwner { param($Type) $this.Owner }
    $acl | Add-Member ScriptMethod GetAccessRules { param($Explicit,$Inherited,$Type) $this.Rules }
    $acl | Add-Member ScriptMethod AddAccessRule { param($Rule) $this.Rules += $Rule }
    return $acl
}
Pass 'split CI effective and OI inherit-only grants' {
    $acl=SplitPolicy
    $rules=@($acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier]))
    if ($rules.Count -ne 2 -or $rules[0].InheritanceFlags -ne 'ContainerInherit' -or $rules[0].PropagationFlags -ne 'None' -or $rules[1].InheritanceFlags -ne 'ObjectInherit' -or $rules[1].PropagationFlags -ne 'InheritOnly') { throw 'Split fixture flags were changed' }
    Assert-AirkDirectoryPolicy $acl (Observation)
}
Pass 'split grants with separate effective parent ACE' {
    $acl=SplitPolicy -DirectoryFlags CIIO
    $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($sid),'FullControl','Allow'))
    Assert-AirkDirectoryPolicy $acl (Observation)
}
Refuse 'split grants lack effective parent' { Assert-AirkDirectoryPolicy (SplitPolicy -DirectoryFlags CIIO) (Observation) }
Refuse 'split grants lack inherited directories' { Assert-AirkDirectoryPolicy (SplitPolicy -DirectoryFlags '') (Observation) }
Refuse 'split grants lack inherited files' { Assert-AirkDirectoryPolicy (SplitPolicy -FileFlags '') (Observation) }
Refuse 'split directory grant cannot propagate' { Assert-AirkDirectoryPolicy (SplitPolicy -DirectoryFlags CINP) (Observation) }
Refuse 'split file grant cannot propagate' { Assert-AirkDirectoryPolicy (SplitPolicy -FileFlags OIIONP) (Observation) }
Refuse 'both split grants cannot propagate' { Assert-AirkDirectoryPolicy (SplitPolicy -DirectoryFlags CINP -FileFlags OIIONP) (Observation) }
Refuse 'split grants still refuse matching deny' {
    $acl=SplitPolicy
    $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new('S-1-1-0'),'Write','Deny'))
    Assert-AirkDirectoryPolicy $acl (Observation)
}
$batchIdentity=Assert-AirkHumanExecution $liveSid
$batchEntries=@([pscustomobject]@{path=$PSCommandPath;root=$root;created=$true},[pscustomobject]@{path=(Join-Path $root 'scripts/dev/assert-human-execution.ps1');root=$root;created=$true})
Pass 'batch checks identity once and each token and ancestor freshly' {
    $script:identityCalls=0; $script:tokenCalls=0; $script:rootAclCalls=0
    $tokenCheck=(Get-Command Assert-AirkCurrentToken).ScriptBlock
    function Assert-AirkHumanExecution { param($ExpectedSid) $script:identityCalls++; $batchIdentity }
    function Assert-AirkCurrentToken { param($Identity) $script:tokenCalls++; & $tokenCheck $Identity }
    function Get-Acl { param($LiteralPath,$ErrorAction) if($LiteralPath -eq $root){$script:rootAclCalls++}; Microsoft.PowerShell.Security\Get-Acl -LiteralPath $LiteralPath -ErrorAction Stop }
    $result=Assert-AirkOutputBatch $batchEntries $liveSid 10
    if (@($result.results | Where-Object {-not $_.passed}).Count -ne 0 -or $script:identityCalls -ne 1 -or $script:tokenCalls -ne 2 -or $script:rootAclCalls -ne 2) { throw 'Batch skipped fresh identity/token/ancestor observations' }
}
Pass 'batch retains all path failures and continues later audits' {
    $missing=[pscustomobject]@{path=(Join-Path $PSScriptRoot 'missing-guard-output-fixture');root=$root;created=$true}
    $result=Assert-AirkOutputBatch @($batchEntries[0],$missing,$batchEntries[1]) $liveSid 10
    if ($result.results.Count -ne 3 -or -not $result.results[0].passed -or $result.results[1].passed -or -not $result.results[2].passed -or $result.results[1].error -ne 'Created output is absent') { throw 'Per-path audit outcomes lost' }
}
Pass 'batch refuses token drift on the affected path' {
    $script:tokenCalls=0
    $tokenCheck=(Get-Command Assert-AirkCurrentToken).ScriptBlock
    function Assert-AirkCurrentToken { param($Identity) $script:tokenCalls++; if($script:tokenCalls -eq 2){throw 'injected token drift'}; & $tokenCheck $Identity }
    $result=Assert-AirkOutputBatch $batchEntries $liveSid 10
    if (-not $result.results[0].passed -or $result.results[1].passed -or $result.results[1].error -ne 'injected token drift') { throw 'Token drift accepted' }
}
Pass 'batch expiry refuses every pending path without path access' {
    function Start-AirkAuditClock { [pscustomobject]@{Elapsed=[pscustomobject]@{TotalSeconds=10}} }
    function Assert-AirkOutputPathObservation { throw 'Expired audit touched path' }
    $result=Assert-AirkOutputBatch $batchEntries $liveSid 10
    if (-not $result.expired -or @($result.results | Where-Object {$_.passed -or $_.error -ne 'Original output audit budget expired'}).Count -ne 0) { throw 'Expired batch accepted' }
}
Pass 'batch malformed row cannot hide later audit' {
    $bad=[pscustomobject]@{path=$PSCommandPath;root=$root;created='true'}
    $result=Assert-AirkOutputBatch @($bad,$batchEntries[1]) $liveSid 10
    if ($result.results[0].passed -or -not $result.results[1].passed) { throw 'Malformed entry accepted or later audit omitted' }
}
Refuse 'batch zero budget refuses before paths' { Assert-AirkOutputBatch $batchEntries $liveSid 0 }
Refuse 'batch widened budget refuses' { Assert-AirkOutputBatch $batchEntries $liveSid 11 }
Refuse 'batch nonfinite budget refuses' { Assert-AirkOutputBatch $batchEntries $liveSid ([double]::NaN) }
Refuse 'batch oversized roster refuses' { Assert-AirkOutputBatch (@($batchEntries[0])*16381) $liveSid 10 }
Refuse 'batch identity failure refuses before path access' {
    function Assert-AirkHumanExecution { throw 'injected identity refusal' }
    Assert-AirkOutputBatch $batchEntries $liveSid 10
}
Pass 'batch same-user token group drift refuses before ACL evaluation' {
    function Assert-AirkHumanExecution {
        param($ExpectedSid)
        $changed=$batchIdentity.PSObject.Copy()
        $changed.TokenSids=@($batchIdentity.TokenSids)+@('S-1-5-999')
        $changed
    }
    $result=Assert-AirkOutputBatch $batchEntries $liveSid 10
    if (@($result.results | Where-Object {$_.passed -or $_.error -ne 'Execution token group roster changed during output audit'}).Count -ne 0) { throw 'Token group drift accepted' }
}
Write-Output "Selected $script:passed; passed $script:passed; failed 0. No filesystem mutations."
