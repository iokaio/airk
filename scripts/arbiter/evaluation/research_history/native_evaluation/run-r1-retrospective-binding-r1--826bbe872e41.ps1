# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$RepoRoot = 'D:\code\Github\airk'
Set-Location -LiteralPath $RepoRoot
$RunRoot = Join-Path $RepoRoot 'artifacts/advanced-split/models/native-r1-retrospective-binding-r1'
if (Test-Path -LiteralPath $RunRoot) { throw 'Fresh private run root required; retain previous diagnostics.' }
New-Item -ItemType Directory -Path $RunRoot | Out-Null
$PrivateTemporaryRoot = Join-Path $RunRoot 'private-temp'
New-Item -ItemType Directory -Path $PrivateTemporaryRoot | Out-Null
$TaskUserSid = [Security.Principal.WindowsIdentity]::GetCurrent().User
$PrivateAcl = [Security.AccessControl.DirectorySecurity]::new()
$PrivateAcl.SetOwner($TaskUserSid)
$PrivateAcl.SetAccessRuleProtection($true, $false)
$PrivateAcl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new(
    $TaskUserSid, [Security.AccessControl.FileSystemRights]::FullControl,
    [Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit',
    [Security.AccessControl.PropagationFlags]::None, [Security.AccessControl.AccessControlType]::Allow))
Set-Acl -LiteralPath $PrivateTemporaryRoot -AclObject $PrivateAcl
$env:TEMP = $PrivateTemporaryRoot
$env:TMP = $PrivateTemporaryRoot
$env:AIRK_PRIVATE_CENSUS_BASE = $PrivateTemporaryRoot
$NativeHarness = Join-Path $RepoRoot 'artifacts/advanced-split/native-harnesses/delivery-06-before-clean/airk_kernel-43b1554a9b9060c4.exe'
$OriginalHarness = Join-Path $RepoRoot 'artifacts/advanced-split/native-fixtures/encoder-campaign-r1/airk_kernel.exe'
$env:AIRK_NATIVE_ENCODER_INPUT = Join-Path $RepoRoot 'artifacts/advanced-split/models/native-encoder-qualification-input-r1.json'
$env:AIRK_NATIVE_RETRIEVAL_INPUT = Join-Path $RepoRoot 'artifacts/advanced-split/models/native-encoder-205-input-r1.json'
$env:AIRK_NATIVE_RETRIEVAL_SOURCE_MANIFEST = Join-Path $RepoRoot 'artifacts/advanced-split/models/native-human-fixtures-r2/manifest.private.json'
$env:AIRK_NATIVE_RETRIEVAL_LEGACY_INPUT = Join-Path $RepoRoot 'artifacts/advanced-split/receipts/delivery-04-native-205-retrieval.json'
$env:AIRK_NATIVE_RETRIEVAL_LEGACY_BINARY = $OriginalHarness
$env:AIRK_NATIVE_RETRIEVAL_OUTPUT = Join-Path $RunRoot 'trained-v2-u8.native-family.json'
$Selector = 'inference_owner::evaluation_tests::retained_r1_trained_u8_capture_binds_its_original_binary_and_unchanged_published_sources'
$Expected = @{
    $NativeHarness = '4616c1ac9075ea4c6cd8098ee9f4a062437b8dc65df4ff619ded2e8a451e1e08'
    $OriginalHarness = '2016caf851b93e7e1ec4267e1fa7fb2a965a4d9e0511c355e5e6a1badcf7f72c'
    $env:AIRK_NATIVE_ENCODER_INPUT = '3ead240726ebdba10577db6778bffbf5232eab10c50f2ae128d589e0a961ab81'
    $env:AIRK_NATIVE_RETRIEVAL_INPUT = '2e74a873125834e17455c40879c0ba161626053fbd39742bcbbc73077d3dd046'
    $env:AIRK_NATIVE_RETRIEVAL_SOURCE_MANIFEST = '2627e54241d8e961cc03c005c02f1655be364462ada16bbafdc0434cf4192a60'
    $env:AIRK_NATIVE_RETRIEVAL_LEGACY_INPUT = '5fb11c1a6d204bb46c6cc59cfecb5be1a7abe73efd882ef99772cc8fd2999b4b'
}
foreach ($Item in $Expected.GetEnumerator()) {
    if ((Get-FileHash -LiteralPath $Item.Key -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Item.Value) { throw "Frozen input changed: $($Item.Key)" }
}
$List = & $NativeHarness $Selector --list --exact 2>&1
$List | Set-Content -LiteralPath (Join-Path $RunRoot 'selector-list.log') -Encoding utf8
if ($LASTEXITCODE -ne 0 -or ($List -join "`n") -notmatch '1 test, 0 benchmarks') { throw 'Exactly one retained native selector required' }
$Command = @($NativeHarness, $Selector, '--ignored', '--exact', '--nocapture', '--test-threads=1')
$Started = [DateTime]::UtcNow.ToString('o')
$Timer = [Diagnostics.Stopwatch]::StartNew()
& $NativeHarness $Selector --ignored --exact --nocapture --test-threads=1 *> (Join-Path $RunRoot 'native.log')
$Code = $LASTEXITCODE
$Timer.Stop()
$Stable = $true
foreach ($Item in $Expected.GetEnumerator()) {
    if ((Get-FileHash -LiteralPath $Item.Key -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Item.Value) { $Stable = $false }
}
$Log = Get-Content -LiteralPath (Join-Path $RunRoot 'native.log') -Raw
$Receipt = [ordered]@{
    schema = 'airk.private-retrospective-binding-run/1'; command = $Command
    source_and_artifact_pins = $Expected; started_at_utc = $Started; finished_at_utc = [DateTime]::UtcNow.ToString('o')
    elapsed_seconds = $Timer.Elapsed.TotalSeconds; returncode = $Code; inputs_unchanged_after = $Stable
    selected_tests = 1; private_temporary_root = $PrivateTemporaryRoot; temporary_acl_sddl = (Get-Acl -LiteralPath $PrivateTemporaryRoot).Sddl
    output = $env:AIRK_NATIVE_RETRIEVAL_OUTPUT
    scope = 'Actual retained native metadata-only retrospective original-capture/binary/publication/source-roster binding. No NN/model query, no human grading, no invented original pre-run seal or performance claim.'
}
$Receipt | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $RunRoot 'run-receipt.json') -Encoding utf8
if ($Code -ne 0 -or -not $Stable -or $Log -notmatch '1 passed; 0 failed') { throw 'Native retrospective binding failed; retained diagnostics, stop without retry.' }
Get-Content -LiteralPath (Join-Path $RunRoot 'run-receipt.json') -Raw
