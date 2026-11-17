# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference='Stop'
$location=Get-Content -Raw artifacts/optimizations/f03-runtime-location.json | ConvertFrom-Json
$env:TEMP=$location.fixtures
$env:TMP=$location.fixtures
$checks=@(
    @{name='install';script='check-service-install.py';args=@('--runtime',$location.runtime)},
    @{name='console';script='check-service-console.py';args=@('--runtime',$location.runtime)},
    @{name='managed';script='check-managed-runtime.py';args=@('--runtime',$location.runtime)},
    @{name='clients';script='check-service-clients.py';args=@('--runtime-dir',$location.runtime,'--files','4','--model','--report',(Join-Path $location.reports 'clients-original.json'))}
)
foreach($check in $checks){
    $script=Join-Path $location.scripts $check.script
    $actual=(Get-FileHash -Algorithm SHA256 -LiteralPath $script).Hash.ToLowerInvariant()
    if($actual -ne $location.script_sha256.($check.script)){throw 'Qualification script copy changed'}
    $arguments=@($script)+$check.args
    $log='artifacts/optimizations/f03-native-'+$check.name+'.log'
    Write-Output ('START native-'+$check.name+' '+[DateTime]::UtcNow.ToString('o'))
    & python @arguments *> $log
    $gateExit=$LASTEXITCODE
    Get-Content -LiteralPath $log -Tail 38
    Write-Output ('EXIT native-'+$check.name+' '+$gateExit+' '+[DateTime]::UtcNow.ToString('o'))
    if($gateExit -ne 0){exit $gateExit}
}
