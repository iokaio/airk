# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
Set-Location D:/code/Github/airk
$env:GIT_CONFIG_COUNT = '1'
$env:GIT_CONFIG_KEY_0 = 'safe.directory'
$env:GIT_CONFIG_VALUE_0 = 'D:/code/Github/airk'
$candidate = 'artifacts/v190/p6a-qualified-runtime'
$control = 'artifacts/v190/p6a-p5-control-copy'
$deadline = [DateTime]::UtcNow.AddMinutes(12)
while (-not (Test-Path "$candidate/airk-build.json")) {
    if ([DateTime]::UtcNow -ge $deadline) { throw 'Qualified pair was not published before campaign admission deadline' }
    Start-Sleep -Seconds 5
}
if (Get-Process cargo,rustc -ErrorAction SilentlyContinue) { throw 'Compiler still running; measurement admission refused' }
$build = Get-Content "$candidate/airk-build.json" -Raw | ConvertFrom-Json
if ($build.source.sha256 -ne 'e261824ba5cf5e93286de5cee5a1b36a68cbc735990b1985bde50bae01ca6ad7') { throw 'Candidate source identity differs' }
$before = (Get-FileHash "$candidate/airk.exe" -Algorithm SHA256).Hash
function Run-Step([string]$Name, [string]$Tool, [string[]]$Arguments) {
    $Name | Set-Content artifacts/v190/p6a-final-campaign-stage.txt
    & $Tool @Arguments *> "artifacts/v190/p6a-final-$Name.log"
    $code = $LASTEXITCODE
    if ($code -ne 0) { throw "$Name failed with exit $code; retain its exact log" }
    Write-Output "$Name passed"
}
$hostProof = @{logical_processors=[Environment]::ProcessorCount; affinity=[Diagnostics.Process]::GetCurrentProcess().ProcessorAffinity.ToInt64(); build_source=$build.source; binary_sha256=$before; started_utc=[DateTime]::UtcNow.ToString('o')}
$hostProof | ConvertTo-Json | Set-Content artifacts/v190/p6a-resumed-measurement-host.json
Run-Step 'control-off-resumed' 'python' @('scripts/benchmark-mcp-investigation.py','owned','--runtime-dir',$control,'--output','artifacts/v190/p6a-control-off-mcp','--label','baseline','--samples','30','--concurrency','1','--verification','off')
Run-Step 'report' 'python' @('scripts/benchmark-mcp-investigation.py','owned','--runtime-dir',$candidate,'--output','artifacts/v190/p6a-report-mcp','--label','candidate','--samples','30','--concurrency','1')
Run-Step 'control-report' 'python' @('scripts/benchmark-mcp-investigation.py','owned','--runtime-dir',$control,'--output','artifacts/v190/p6a-control-report-mcp','--label','baseline','--samples','30','--concurrency','1')
if ((Get-FileHash "$candidate/airk.exe" -Algorithm SHA256).Hash -ne $before) { throw 'Candidate binary changed during campaign' }
Run-Step 'gates' 'python' @('artifacts/v190/p6a-evaluate-gates.py')
'complete' | Set-Content artifacts/v190/p6a-final-campaign-stage.txt
