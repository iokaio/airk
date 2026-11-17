# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$base = 'D:/code/Github/airk/artifacts/local/one-airk-1.8.14'
$fixtureBase = (Get-Content -LiteralPath "$base/completion-editor-fixture-base-r26.json" -Raw | ConvertFrom-Json).path
$fixture = Join-Path $fixtureBase 'legacy/fixture.json'
if (-not (Test-Path -LiteralPath $fixture -PathType Leaf)) { throw 'Missing completed producer fixture' }
$env:AIRK_EDITOR_LEGACY_FIXTURE = $fixture
$env:AIRK_EDITOR_SCENARIOS = 'same,different,multi,usage,untrusted,legacy'
$vsix = "$base/windows-debug-vsix-r26/publish/airk-1.8.14-win32-x64.vsix"
if ((Get-FileHash -LiteralPath $vsix -Algorithm SHA256).Hash.ToLowerInvariant() -ne 'e91f564a2db03a3e97fa1832aeb9f6075bb55be5b189c5f96720fe2bca8a8272') { throw 'Changed VSIX identity' }
node scripts/check-native-editor.cjs 'C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/Code.exe' 'C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/c4453f8ea4/resources/app/out/cli.js' $vsix
exit $LASTEXITCODE
