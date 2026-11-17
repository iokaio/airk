# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference='Stop'
Set-Location 'D:/code/Github/airk'
$airkPrivateBase=Join-Path $env:TEMP ('airk-release187-validation-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $airkPrivateBase | Out-Null
$airkUser=[Security.Principal.WindowsIdentity]::GetCurrent().User
$airkAcl=[Security.AccessControl.DirectorySecurity]::new()
$airkAcl.SetOwner($airkUser)
$airkAcl.SetAccessRuleProtection($true,$false)
$airkAcl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($airkUser,'FullControl','ContainerInherit,ObjectInherit','None','Allow'))
Set-Acl -LiteralPath $airkPrivateBase -AclObject $airkAcl
$airkObserved=Get-Acl -LiteralPath $airkPrivateBase
if ($airkObserved.Access.Count -ne 1 -or $airkObserved.Owner -ne [Security.Principal.WindowsIdentity]::GetCurrent().Name) { throw 'Fresh fixture ACL is not canonical owner-only' }
$airkPrivateBase | Set-Content 'artifacts/release-1.8.7-review/private-base.txt'
$airkObserved.Sddl | Set-Content 'artifacts/release-1.8.7-review/private-sddl.txt'
$env:AIRK_PRIVATE_CENSUS_BASE=$airkPrivateBase
$env:PATH="$env:USERPROFILE/.cargo/bin;C:/Program Files/Git/cmd;"+$env:PATH
$env:CARGO_BUILD_JOBS='4'
& cargo test -p airk-cli --test native_owner_recovery --all-features --locked --offline -- --test-threads=2 *> 'artifacts/release-1.8.7-review/recovery-targeted.log'
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Output 'Actual-process native-owner recovery: passed with canonical owner-only temporary namespace.'
& cargo test --workspace --all-features --locked --offline -- --test-threads=2 *> 'artifacts/release-1.8.7-review/workspace-final.log'
$airkExit=$LASTEXITCODE
@{exit_code=$airkExit;fixture=$airkPrivateBase;finished=[DateTime]::UtcNow.ToString('o')} | ConvertTo-Json | Set-Content 'artifacts/release-1.8.7-review/workspace-final-result.json'
exit $airkExit
