# SPDX-License-Identifier: Apache-2.0
# Artifact-only prepared runner. Execute only after the release VSIX is sealed and reviewed.
param(
    [Parameter(Mandatory=$true)][ValidatePattern('^[a-f0-9]{64}$')][string]$CandidateSha256,
    [string]$LegacyFixture = ''
)
$ErrorActionPreference = 'Stop'
$repo = 'D:/code/Github/airk'
$evidence = "$repo/artifacts/local/one-airk-1.8.14"
$editor = 'C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/Code.exe'
# This installed Code version stores app resources beneath its update hash directory.
$cli = 'C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/c4453f8ea4/resources/app/out/cli.js'
$baseline = "$repo/artifacts/local/1.8.12-explorer-20261107/vscode-final/publish/airk-1.8.12-win32-x64.vsix"
$baselineSha = '64609d630765a161fd84d887daa4253ea0ea387c33fcf0805e1e9cb7f1c8d54b'
$candidate = "$evidence/windows-release-vsix-r1/publish/airk-1.8.14-win32-x64.vsix"
$old = "$repo/artifacts/local/airk-1.8.7-f75abfa6/vscode/publish/airk-1.8.7-win32-x64.vsix"
$oldSha = 'fd1da068ff494bbe1a51dceef708b7afe0e2e94c0a9c9bce11206c9451c0051a'
function Require-Hash([string]$File,[string]$Expected) {
    $item=Get-Item -LiteralPath $File
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "Expected plain input file: $File" }
    $actual=(Get-FileHash -LiteralPath $File -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -cne $Expected) { throw "Reviewed input changed: $File" }
}
Require-Hash $editor '51dfee70fd38d8cc3e204f9a304f6fb6700dfa5147223c59d8c37e8176fb9b64'
Require-Hash $cli '1fa196655d93959322f3d570b35c3def6d6cbb201cb1d776435a7dd1f5136fdd'
Require-Hash $baseline $baselineSha
Require-Hash $candidate $CandidateSha256
$packageReceipt=Get-Content -LiteralPath "$evidence/windows-release-vsix-r1/publish/manifest.json" -Raw | ConvertFrom-Json
if ($packageReceipt.version -cne '1.8.14' -or $packageReceipt.platform -cne 'win32-x64' -or $packageReceipt.archive_sha256 -cne $CandidateSha256 -or $packageReceipt.archive -cne 'airk-1.8.14-win32-x64.vsix') { throw 'Candidate package receipt differs from reviewed Windows VSIX' }
$privateBase=(Get-Content -LiteralPath "$evidence/user-private-fixture-base.json" -Raw | ConvertFrom-Json).path
if (-not (Test-Path -LiteralPath $privateBase -PathType Container)) { throw 'Existing owned private fixture base unavailable' }
$env:TEMP=$privateBase
$env:TMP=$privateBase
$env:AIRK_PRIVATE_CENSUS_BASE=$privateBase
$env:AIRK_EDITOR_SCENARIOS='same,different,multi,usage,untrusted,legacy'
if ($LegacyFixture) {
    $env:AIRK_EDITOR_LEGACY_FIXTURE=(Get-Item -LiteralPath $LegacyFixture).FullName
} else {
    Remove-Item Env:AIRK_EDITOR_LEGACY_FIXTURE -ErrorAction SilentlyContinue
}
Set-Location -LiteralPath $repo
$node=(Get-Command node -CommandType Application).Source
function Invoke-Gate([string]$Name,[string]$Executable,[string[]]$Arguments) {
    $log="$evidence/$Name.log"
    $receipt="$evidence/$Name.json"
    if ((Test-Path -LiteralPath $log) -or (Test-Path -LiteralPath $receipt)) { throw "Refusing to overwrite gate evidence: $Name" }
    Require-Hash $candidate $CandidateSha256
    $timer=[Diagnostics.Stopwatch]::StartNew()
    & $Executable @Arguments *> $log
    $code=$LASTEXITCODE
    $timer.Stop()
    @{
        executable=$Executable;arguments=$Arguments;exit_code=$code;elapsed_seconds=$timer.Elapsed.TotalSeconds
        candidate=$candidate;candidate_sha256=$CandidateSha256;baseline=$baseline;baseline_sha256=$baselineSha
        editor=$editor;cli=$cli;execution='actual installed VS Code; harness-owned isolated profiles and extension directories'
        scenario_selection=$env:AIRK_EDITOR_SCENARIOS;legacy_fixture=$LegacyFixture
    } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $receipt
    Write-Output "$Name exit=$code elapsed=$($timer.Elapsed.TotalSeconds)"
    if ($code -ne 0) { Get-Content -LiteralPath $log -Tail 45; exit $code }
    Require-Hash $candidate $CandidateSha256
}
# Both scripts create owned private profiles; neither targets the normal editor profile.
Invoke-Gate 'windows-release-explorer-hot-update-r1' $node @('scripts/check-explorer-hot-update.cjs',$editor,$cli,$baseline,$baselineSha,$candidate,$CandidateSha256)
Invoke-Gate 'windows-release-native-editor-r1' $node @('scripts/check-native-editor.cjs',$editor,$cli,$candidate)
# The native-editor legacy scenario remains explicitly unqualified without an
# authentic managed-upgrade receipt passed through -LegacyFixture. Do not treat
# a successful process exit as proof every scenario was qualified: inspect summary.json.
# Genuine Windows 1.8.7 is retained at $old (SHA $oldSha), but the existing
# check-vsix-upgrade-editor.cjs is explicitly Apple Silicon-only and pins a Mac
# release asset. No Windows old-restart gate is executed or claimed by this runner.
if (Test-Path -LiteralPath $old) { Require-Hash $old $oldSha }
Write-Output 'Windows 1.8.7 explicit-restart qualification is not run: existing harness is Mac-only.'

