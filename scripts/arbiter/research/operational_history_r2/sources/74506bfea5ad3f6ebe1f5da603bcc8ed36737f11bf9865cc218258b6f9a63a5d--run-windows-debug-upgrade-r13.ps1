# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$env:PATH = 'C:\Users\tdotn\.cargo\bin;D:\code\Github\airk\artifacts\tools\nextest;' + $env:PATH
$evidence = 'D:/code/Github/airk/artifacts/local/one-airk-1.8.14'
$target = 'D:/code/Github/airk/artifacts/v190/p10-target'
$staticEnv = (Get-Content "$evidence/static-environment-supplement.json" -Raw | ConvertFrom-Json).environment
foreach ($property in $staticEnv.PSObject.Properties) {
    [Environment]::SetEnvironmentVariable($property.Name, [string]$property.Value, 'Process')
}
# A fresh short fixture parent avoids the genuine predecessor's MAX_PATH boundary.
# No existing directory ACL or production runtime is modified.
$workspace = (Get-Item -LiteralPath 'D:/code/Github/airk').FullName
$shortParent = Get-Item -LiteralPath (Join-Path $workspace 'target')
if (($shortParent.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Fixture parent must not be a reparse point' }
$privateBase = Join-Path $shortParent.FullName ('ud-' + [Guid]::NewGuid().ToString('N').Substring(0,8))
if (Test-Path -LiteralPath $privateBase) { throw 'Fresh fixture collision' }
New-Item -ItemType Directory -Path $privateBase -ErrorAction Stop | Out-Null
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
$acl = New-Object Security.AccessControl.DirectorySecurity
$acl.SetOwner($sid)
$acl.SetAccessRuleProtection($true, $false)
$rule = New-Object Security.AccessControl.FileSystemAccessRule($sid, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
$acl.AddAccessRule($rule)
Set-Acl -LiteralPath $privateBase -AclObject $acl
$verifiedAcl = Get-Acl -LiteralPath $privateBase
if (-not $verifiedAcl.AreAccessRulesProtected -or $verifiedAcl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid.Value) { throw 'Private fixture ACL ownership failed' }
$rules = @($verifiedAcl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier]))
if ($rules.Count -ne 1 -or $rules[0].IdentityReference.Value -ne $sid.Value -or $rules[0].IsInherited -or $rules[0].AccessControlType -ne 'Allow' -or $rules[0].FileSystemRights -ne 'FullControl') { throw 'Private fixture ACL has unexpected access' }
$baseReceipt = "$evidence/user-private-short-upgrade-r13.json"
if (Test-Path -LiteralPath $baseReceipt) { throw 'Refusing to overwrite short fixture receipt' }
@{schema=1;path=$privateBase;owner=$sid.Value;sddl=$verifiedAcl.Sddl;purpose='isolated authentic schema1 upgrade; short path, strict owner-only ACL'} | ConvertTo-Json | Set-Content -LiteralPath $baseReceipt
$env:TEMP = $privateBase
$env:TMP = $privateBase
$env:AIRK_PRIVATE_CENSUS_BASE = $privateBase
$env:AIRK_TEST_MODEL_DIR = 'D:/code/Github/airk/target/local-vector-model/minilm-l6-v2-u8-v1'
$env:CARGO_INCREMENTAL = '1'
$env:GIT_CONFIG_COUNT = '1'
$env:GIT_CONFIG_KEY_0 = 'safe.directory'
$env:GIT_CONFIG_VALUE_0 = 'D:/code/Github/airk'
$cargo = 'C:/Users/tdotn/.cargo/bin/cargo.exe'

function Invoke-Gate {
    param([string]$Name, [string]$Executable, [string[]]$Arguments)
    $log = "$evidence/$Name.log"
    $receipt = "$evidence/$Name.json"
    if ((Test-Path -LiteralPath $log) -or (Test-Path -LiteralPath $receipt)) {
        throw "Refusing to overwrite gate evidence: $Name"
    }
    $timer = [Diagnostics.Stopwatch]::StartNew()
    & $Executable @Arguments *> $log
    $code = $LASTEXITCODE
    @{
        executable=$Executable; arguments=$Arguments; target=$target
        incremental='on'; elapsed_seconds=$timer.Elapsed.TotalSeconds; exit_code=$code
        execution='normal user; fresh short owner-only fixture base; unchanged sealed r7 runtime; no Cargo'
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $receipt
    Write-Output "$Name exit=$code elapsed=$($timer.Elapsed.TotalSeconds)"
    if ($code -ne 0) {
        Get-Content -LiteralPath $log -Tail 45
        exit $code
    }
}

$env:AIRK_PROCESS_DIAGNOSTICS = '1'
$env:AIRK_NATIVE_DIAGNOSTIC_DIR = Join-Path $privateBase 'upgrade-diagnostics-r13'
New-Item -ItemType Directory -Path $env:AIRK_NATIVE_DIAGNOSTIC_DIR -ErrorAction Stop | Out-Null
$env:CARGO_TARGET_DIR = $target
$debug = "$evidence/windows-debug-r8/runtime"
Invoke-Gate 'windows-debug-schema1-upgrade-r13' 'python' @('scripts/check-runtime-upgrade.py','--runtime',$debug,'--previous-runtime','artifacts/local/1.8.12-explorer-20261107/runtime','--source-state','indexed','--fixture-setup','reviewed','--semantic-old','--output',"$evidence/windows-debug-schema1-upgrade-result-r13.json")










