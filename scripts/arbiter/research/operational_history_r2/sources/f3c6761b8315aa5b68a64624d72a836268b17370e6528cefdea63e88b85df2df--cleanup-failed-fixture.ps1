# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$fixtureOwner = 'D:\code\Github\airk\artifacts\one-process\native-fixtures-private\.tmpNfRH9B'
$expectedImage = '\\?\D:\code\Github\airk\artifacts\one-process\native-fixtures-private\.tmpNfRH9B\install\runtimes\33523da21aea4c76bade09ab09a20a55f4b71874aedb73bd81e67cc5b2217f55\airk.exe'
$observed = Get-CimInstance Win32_Process -Filter 'ProcessId = 16640'
if ($observed.ExecutablePath -cne $expectedImage -or $observed.CommandLine -notlike '*"serve" "--runtime-dir"*' -or $observed.CreationDate.ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss') -ne '2026-11-11T01:59:46') { throw 'Fixture process identity changed' }
$ownedProcess = Get-Process -Id 16640
$null = $ownedProcess.Handle
$control = Join-Path $fixtureOwner 'state\control'
$proof = [ordered]@{ fixture=$fixtureOwner; pid=16640; created=$observed.CreationDate; image=$expectedImage; fixtureRetained=$true; originalLifecycleCleanupComplete=$false; reason='Failed test destructor partially removed install metadata; runtime inspect preflight fails. Graceful service shutdown is separate recovery evidence.' }
$healthText = '"health"' | & $expectedImage service rpc --runtime-dir $control --stdin
if ($LASTEXITCODE -ne 0) { throw 'Fixture health unavailable' }
$health = $healthText | ConvertFrom-Json
$imageHash = (Get-FileHash -LiteralPath $expectedImage -Algorithm SHA256).Hash.ToLowerInvariant()
if ($health.status -ne 'health' -or $health.observation.executable_sha256 -ne $imageHash -or $health.observation.runtime_sha256 -ne '33523da21aea4c76bade09ab09a20a55f4b71874aedb73bd81e67cc5b2217f55') { throw 'Fixture service identity changed' }
$proof.health = $health
$proof | ConvertTo-Json -Depth 15 | Set-Content -LiteralPath (Join-Path $fixtureOwner 'manual-graceful-cleanup.json')
$result = & $expectedImage service shutdown --runtime-dir $control 2>&1
$proof.shutdownExitCode = $LASTEXITCODE
$proof.shutdownOutput = "$result"
$exited = $ownedProcess.WaitForExit(45000)
$proof.exactProcessExited = $exited
$proof | ConvertTo-Json -Depth 15 | Set-Content -LiteralPath (Join-Path $fixtureOwner 'manual-graceful-cleanup.json')
if (-not $exited -or $proof.shutdownExitCode -ne 0) { throw 'Graceful fixture shutdown incomplete; retained evidence' }
Write-Output 'Exact fixture PID 16640 exited after graceful service shutdown; original lifecycle receipt remains incomplete; fixture retained.'
