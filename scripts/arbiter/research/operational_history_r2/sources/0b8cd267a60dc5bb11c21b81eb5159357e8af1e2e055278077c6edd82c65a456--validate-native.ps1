# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
Set-Location 'D:/code/Github/airk'
$env:PATH = "$env:USERPROFILE/.cargo/bin;C:/Program Files/Git/cmd;" + $env:PATH
$airkPrivateBase = Join-Path (Get-Location) ('artifacts/release-1.8.7-review/private-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $airkPrivateBase | Out-Null
$airkIdentity = [Security.Principal.WindowsIdentity]::GetCurrent().Name
& icacls.exe $airkPrivateBase /inheritance:r /grant:r "${airkIdentity}:(OI)(CI)F" *> 'artifacts/release-1.8.7-review/private-acl.log'
if ($LASTEXITCODE -ne 0) { throw 'Cannot prepare private test namespace' }
$env:AIRK_PRIVATE_CENSUS_BASE = $airkPrivateBase
$env:CARGO_BUILD_JOBS = '4'
$airkResults = @()
foreach ($airkCheck in @(
    @{Name='format';Args=@('fmt','--all','--check')},
    @{Name='clippy';Args=@('clippy','--workspace','--all-targets','--all-features','--locked','--offline','--','-D','warnings')},
    @{Name='workspace';Args=@('test','--workspace','--all-features','--locked','--offline','--','--test-threads=2')}
)) {
    $airkStarted = [DateTime]::UtcNow
    & cargo @($airkCheck.Args) *> "artifacts/release-1.8.7-review/$($airkCheck.Name).log"
    $airkExit = $LASTEXITCODE
    $airkResults += @{name=$airkCheck.Name;exit_code=$airkExit;started=$airkStarted.ToString('o');finished=[DateTime]::UtcNow.ToString('o')}
    $airkResults | ConvertTo-Json -Depth 5 | Set-Content 'artifacts/release-1.8.7-review/native-results.json'
    Write-Output "$($airkCheck.Name): exit $airkExit"
    if ($airkExit -ne 0) { exit $airkExit }
}
