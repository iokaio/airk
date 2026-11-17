# SPDX-License-Identifier: Apache-2.0
# Read-only Windows preflight. Dot-source, then call before every write batch and
# audit every created output before consuming it. This is a metadata snapshot,
# not a sandbox: callers must enumerate outputs/caches, preserve child tokens,
# and avoid concurrent path/ACL changes. No account or ACL repair is performed.

function Assert-AirkIdentityObservation {
    param([Parameter(Mandatory)]$Observation, [Parameter(Mandatory)][string]$ExpectedSid)
    if ($Observation.UserSid -ne $ExpectedSid) { throw 'Effective user does not match the expected human SID' }
    if ($Observation.OwnerSid -ne $ExpectedSid) { throw 'Token default owner does not match the human SID' }
    if ($Observation.SessionId -le 0) { throw 'No interactive execution session' }
    $owners = @($Observation.InteractiveOwnerSids | Sort-Object -Unique)
    if ($owners.Count -ne 1 -or $owners[0] -ne $ExpectedSid) { throw 'Current-session interactive owner is absent or ambiguous' }
}

function Assert-AirkHumanExecution {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$ExpectedSid)
    if (-not $IsWindows) { throw 'This guard requires Windows; verify other hosts separately' }
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    try {
        $session = [Diagnostics.Process]::GetCurrentProcess().SessionId
        $owners = @(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe' AND SessionId = $session" -ErrorAction Stop | ForEach-Object {
            $result = Invoke-CimMethod -InputObject $_ -MethodName GetOwnerSid -ErrorAction Stop
            if ($result.ReturnValue -ne 0 -or -not $result.Sid) { throw 'Cannot establish interactive Explorer owner' }
            $result.Sid
        })
        $observation = [pscustomobject]@{
            UserName = $identity.Name; UserSid = $identity.User.Value
            OwnerSid = $identity.Owner.Value; SessionId = $session
            InteractiveOwnerSids = $owners
            TokenSids = @($identity.User.Value) + @($identity.Groups | ForEach-Object Value)
            ObservedUtc = [DateTime]::UtcNow.ToString('o')
        }
        Assert-AirkIdentityObservation $observation $ExpectedSid
        return $observation
    } finally { $identity.Dispose() }
}

function Assert-AirkDirectoryPolicy {
    param([Parameter(Mandatory)]$Acl, [Parameter(Mandatory)]$Identity)
    $sid = $Identity.UserSid
    if ($Acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid) { throw 'Output parent is foreign-owned' }
    $rules = @($Acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier]))
    $full = [Security.AccessControl.FileSystemRights]::FullControl
    # Conservative sufficient policy, not a general Windows Authz evaluator:
    # require a direct human full-control grant for parent and descendants and
    # refuse any applicable token deny, including deny-only group membership.
    $parentGrant = $false; $childDirectoryGrant = $false; $childFileGrant = $false
    foreach ($rule in $rules) {
        $ruleSid = $rule.IdentityReference.Value
        if ($rule.AccessControlType -eq 'Deny' -and ($ruleSid -in $Identity.TokenSids -or $ruleSid -eq 'S-1-3-4')) {
            throw 'Output policy contains an applicable deny; manual access review required'
        }
        if ($ruleSid -ne $sid -or $rule.AccessControlType -ne 'Allow' -or ($rule.FileSystemRights -band $full) -ne $full) { continue }
        if (($rule.PropagationFlags -band [Security.AccessControl.PropagationFlags]::InheritOnly) -eq 0) { $parentGrant = $true }
        # CI becomes effective on child directories; OI becomes effective on
        # child files and propagates through directories as inherit-only. The
        # grants may be separate ACEs, including an OI/InheritOnly file grant.
        # NoPropagate cannot cover further descendants and is insufficient.
        # https://learn.microsoft.com/en-us/windows/win32/secauthz/ace-inheritance-rules
        if (($rule.PropagationFlags -band [Security.AccessControl.PropagationFlags]::NoPropagateInherit) -eq 0) {
            if (($rule.InheritanceFlags -band [Security.AccessControl.InheritanceFlags]::ContainerInherit) -ne 0) { $childDirectoryGrant = $true }
            if (($rule.InheritanceFlags -band [Security.AccessControl.InheritanceFlags]::ObjectInherit) -ne 0) { $childFileGrant = $true }
        }
    }
    if (-not $parentGrant -or -not $childDirectoryGrant -or -not $childFileGrant) { throw 'No explicit human full-control grant covering parent and inherited outputs' }
}

function Get-AirkCheckedPath {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string]$Root)
    foreach ($candidate in @($Path, $Root)) {
        if ($candidate -notmatch '^[A-Za-z]:[\\/]' -or $candidate.Substring(2).Contains(':') -or $candidate -match '[*?<>|"\x00-\x1f]') { throw 'Use an absolute local path without streams or wildcards' }
        foreach ($part in ($candidate.Substring(3) -split '[\\/]')) {
            if ($part -match '^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])([.]|$)') { throw 'Windows device name refused' }
            if ($part -notin @('', '.', '..') -and ($part.EndsWith('.') -or $part.EndsWith(' '))) { throw 'Ambiguous Windows path component' }
        }
    }
    $full = [IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
    $boundary = [IO.Path]::GetFullPath($Root).TrimEnd('\', '/')
    if ($boundary -match '^[A-Za-z]:$') { throw 'Approved root must be narrower than a drive' }
    if ($full -ne $boundary -and -not $full.StartsWith($boundary + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Output path escapes the approved root' }
    $cursor = $full
    while ($cursor) {
        if (Test-Path -LiteralPath $cursor -ErrorAction Stop) {
            $item = Get-Item -LiteralPath $cursor -Force -ErrorAction Stop
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Reparse point refused: $cursor" }
        }
        $cursor = [IO.Path]::GetDirectoryName($cursor)
    }
    return $full
}

function Assert-AirkOutputPathObservation {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string]$Root, [Parameter(Mandatory)]$Identity)
    $ExpectedSid = $Identity.UserSid
    Assert-AirkCurrentToken $Identity
    $full = Get-AirkCheckedPath $Path $Root
    # Require the immediate parent to exist; create directories one level at a
    # time and audit them before using them as parents of further outputs.
    $parent = [IO.Path]::GetDirectoryName($full)
    if (-not (Test-Path -LiteralPath $parent -PathType Container -ErrorAction Stop)) { throw 'Immediate output parent must already exist' }
    $boundary = [IO.Path]::GetFullPath($Root).TrimEnd('\', '/')
    if ($boundary -match '^[A-Za-z]:$') { throw 'Approved root must be narrower than a drive' }
    if ($full -eq $boundary) { throw 'Output must be inside, not equal to, the approved root' }
    $cursor = $parent
    while ($true) {
        Assert-AirkDirectoryPolicy (Get-Acl -LiteralPath $cursor -ErrorAction Stop) $identity
        if ($cursor -eq $boundary) { break }
        $cursor = [IO.Path]::GetDirectoryName($cursor)
    }
    if (Test-Path -LiteralPath $full -ErrorAction Stop) {
        if ((Get-Acl -LiteralPath $full -ErrorAction Stop).GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $ExpectedSid) { throw 'Existing output is foreign-owned' }
    }
    [pscustomobject]@{ Path = $full; Parent = $parent; Identity = $identity }
}

# A batch is one read-only process. Reobserve its effective/default-owner token
# for every path; never reuse ACL/path observations across entries or batches.
function Assert-AirkCurrentToken {
    param([Parameter(Mandatory)]$Identity)
    $current = [Security.Principal.WindowsIdentity]::GetCurrent()
    try {
        if ($current.User.Value -ne $Identity.UserSid -or $current.Owner.Value -ne $Identity.UserSid -or [Diagnostics.Process]::GetCurrentProcess().SessionId -ne $Identity.SessionId) { throw 'Execution token changed during output audit' }
        $currentSids = @(@($current.User.Value) + @($current.Groups | ForEach-Object Value) | Sort-Object -Unique)
        $originalSids = @($Identity.TokenSids | Sort-Object -Unique)
        if (@(Compare-Object $originalSids $currentSids).Count -ne 0) { throw 'Execution token group roster changed during output audit' }
    } finally { $current.Dispose() }
}

function Assert-AirkOutputParent {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string]$Root, [Parameter(Mandatory)][string]$ExpectedSid)
    $identity = Assert-AirkHumanExecution $ExpectedSid
    Assert-AirkOutputPathObservation -Path $Path -Root $Root -Identity $identity
}

function Start-AirkAuditClock { [Diagnostics.Stopwatch]::StartNew() }

function Assert-AirkOutputBatch {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object[]]$Entries, [Parameter(Mandatory)][string]$ExpectedSid, [Parameter(Mandatory)][double]$BudgetSeconds)
    if ($Entries.Count -lt 1 -or $Entries.Count -gt 16380 -or -not [double]::IsFinite($BudgetSeconds) -or $BudgetSeconds -le 0 -or $BudgetSeconds -gt 10) { throw 'Invalid bounded output audit batch' }
    $clock = Start-AirkAuditClock
    $identity = Assert-AirkHumanExecution $ExpectedSid
    $results = @(for ($index = 0; $index -lt $Entries.Count; $index++) {
        $entry = $Entries[$index]
        $errorText = $null
        try {
            if ($clock.Elapsed.TotalSeconds -ge $BudgetSeconds) { throw 'Original output audit budget expired' }
            if (@($entry.PSObject.Properties.Name).Count -ne 3 -or $null -eq $entry.path -or $null -eq $entry.root -or $entry.created -isnot [bool]) { throw 'Invalid output audit entry' }
            $check = Assert-AirkOutputPathObservation -Path $entry.path -Root $entry.root -Identity $identity
            if ($entry.created) {
                if (-not (Test-Path -LiteralPath $check.Path -ErrorAction Stop)) { throw 'Created output is absent' }
                if ((Get-Acl -LiteralPath $check.Path -ErrorAction Stop).GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $ExpectedSid) { throw 'Created output has a foreign owner' }
            }
            if ($clock.Elapsed.TotalSeconds -ge $BudgetSeconds) { throw 'Original output audit budget expired' }
        } catch { $errorText = $_.Exception.Message }
        [pscustomobject]@{ index=$index; passed=($null -eq $errorText); error=$errorText }
    })
    $elapsed = $clock.Elapsed.TotalSeconds
    [pscustomobject]@{ schema='airk.human-output-audit/1'; results=$results; elapsed_seconds=$elapsed; expired=($elapsed -ge $BudgetSeconds) }
}
function Assert-AirkCreatedOutput {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string]$Root, [Parameter(Mandatory)][string]$ExpectedSid)
    $check = Assert-AirkOutputParent -Path $Path -Root $Root -ExpectedSid $ExpectedSid
    if (-not (Test-Path -LiteralPath $check.Path -ErrorAction Stop)) { throw 'Created output is absent' }
    if ((Get-Acl -LiteralPath $check.Path -ErrorAction Stop).GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $ExpectedSid) { throw 'Created output has a foreign owner' }
    return $check
}
