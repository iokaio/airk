# SPDX-License-Identifier: Apache-2.0
"""Windows entrypoint contracts without starting a native build."""
import shutil
import subprocess
import unittest
import os
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


class WindowsStaticEntrypoints(unittest.TestCase):
    @unittest.skipUnless(os.name == 'nt' and shutil.which('pwsh'), 'Actual Windows wrapper control flow')
    def test_current_extension_compiles_before_packaging_contracts_on_a_stale_checkout(self):
        # Run the actual wrapper with inert tools and private output. The
        # packaging double rejects the stale extension.js until npm test has
        # produced the newly required module, matching the real closure gate.
        with tempfile.TemporaryDirectory(prefix='airk-wrapper-order-') as temporary:
            root = Path(temporary)
            (root / 'scripts').mkdir()
            shutil.copyfile(ROOT / 'scripts/build-windows.ps1', root / 'scripts/build-windows.ps1')
            (root / 'scripts/check-repo.ps1').write_text('$global:LASTEXITCODE=0\n', encoding='utf-8')
            output = root / 'packages/vscode/out'
            output.mkdir(parents=True)
            (output / 'extension.js').write_text('// stale predecessor output\n', encoding='utf-8')
            script = r'''
$ErrorActionPreference='Stop'
$reviewRoot=$env:AIRK_WRAPPER_REVIEW_ROOT
$global:reviewTrace=[Collections.Generic.List[string]]::new()
function Invoke-ReviewChild([string]$Tool,[string[]]$Arguments) {
    if($Arguments.Count -and $Arguments[0] -eq 'scripts/static-runtime-environment.py') {
        $boundary=[Array]::IndexOf($Arguments,'--')
        $Tool=$Arguments[$boundary+1]
        $Arguments=@($Arguments[($boundary+2)..($Arguments.Length-1)])
    }
    $global:reviewTrace.Add($Tool+' '+($Arguments -join ' '))
    $global:LASTEXITCODE=0
    if($Tool -eq 'npm.cmd' -and $Arguments[0] -eq 'test') {
        [IO.File]::WriteAllText((Join-Path $reviewRoot 'packages/vscode/out/httpIssuerRotation.js'),'// current compiled fixture')
    }
    if($Arguments.Count -gt 1 -and $Arguments[0] -eq '-m' -and $Arguments[1] -eq 'unittest') {
        if(-not(Test-Path -LiteralPath (Join-Path $reviewRoot 'packages/vscode/out/httpIssuerRotation.js'))) {
            $global:LASTEXITCODE=19
        }
    }
    if($Arguments.Count -and $Arguments[0] -eq 'scripts/package-extension.py') {
        $directory=$Arguments[[Array]::IndexOf($Arguments,'--output')+1]
        [IO.Directory]::CreateDirectory((Join-Path $directory 'publish'))>$null
        [IO.File]::WriteAllText((Join-Path $directory 'publish/manifest.json'),'{"archive":"fictional.vsix"}')
    }
}
function cargo { Invoke-ReviewChild 'cargo' $args }
function rustup { Invoke-ReviewChild 'rustup' $args }
function node { Invoke-ReviewChild 'node' $args }
function npm.cmd { Invoke-ReviewChild 'npm.cmd' $args }
function py { Invoke-ReviewChild 'py' $args }
function python { Invoke-ReviewChild 'python' $args }
function git { Invoke-ReviewChild 'git' $args }
& (Join-Path $reviewRoot 'scripts/build-windows.ps1')
$trace=$global:reviewTrace -join "`n"
if(($global:reviewTrace | Where-Object { $_ -match '^npm.cmd test ' }).Count -ne 1) { throw 'Expected exactly one current npm compile/test stage' }
if($global:reviewTrace | Where-Object { $_ -match '^npm.cmd run compile ' }) { throw 'Duplicate extension compile' }
if($trace.IndexOf('npm.cmd test ') -gt $trace.IndexOf('-m unittest ')) { throw 'Packaging contracts ran before current compile' }
if($trace.IndexOf('-m unittest ') -gt $trace.IndexOf('scripts/package-native.py')) { throw 'Packaging ran before contract gates' }
'''
            result = subprocess.run([shutil.which('pwsh'), '-NoProfile', '-Command', script], cwd=ROOT,
                                    env={**os.environ, 'AIRK_WRAPPER_REVIEW_ROOT': str(root)},
                                    text=True, capture_output=True, timeout=30)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_provisioning_is_explicit_and_precedes_builder(self):
        for name in ("build-windows.ps1", "ci-local.ps1"):
            source = (ROOT / "scripts" / name).read_text()
            with self.subTest(script=name):
                self.assertNotIn("scripts/provision-onnx-runtime.py", source)
                self.assertIn("else { $airkStaticArgs += '--verify' }", source)
                self.assertIn("if ($DownloadInputs) { $airkStaticArgs += @('--download', '--download-tools', '--build', '--parallel', '2') }", source)
                self.assertIn('target/static-onnx-runtime/$airkTarget', source)
                self.assertIn("scripts/static-runtime-environment.py", source)
                self.assertLess(source.index('Invoke-AirkTool $airkPython $airkStaticArgs') if name == 'build-windows.ps1' else source.index('Invoke-AirkTool python $airkStaticArgs'), source.rindex('scripts/build-service-runtime.py'))

    @unittest.skipUnless(shutil.which("pwsh"), "PowerShell 7 unavailable")
    def test_build_wrapper_routes_cargo_and_runtime_only_through_verified_child_environment(self):
        # Extract the actual function via PowerShell's parser; never execute the
        # entrypoint's provisioning/build body. Replace the child with a recorder.
        script = r"""
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $PWD 'scripts/build-windows.ps1'), [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw 'Invalid PowerShell source' }
$fn = $ast.Find({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Invoke-AirkTool'}, $true)
Invoke-Expression $fn.Extent.Text
function Record-Child { $script:captured = @($args); $global:LASTEXITCODE = 0 }
$airkPython = 'Record-Child'; $airkStaticEnvironment = 'owned report.json'
Invoke-AirkTool cargo @('test', '--locked')
if (($captured -join '|') -ne 'scripts/static-runtime-environment.py|--environment-file|owned report.json|--|cargo|test|--locked') { throw 'Cargo escaped static environment wrapper' }
Invoke-AirkTool Record-Child @('scripts/build-service-runtime.py', '--destination', 'owned output')
if (($captured -join '|') -ne 'scripts/static-runtime-environment.py|--environment-file|owned report.json|--|Record-Child|scripts/build-service-runtime.py|--destination|owned output') { throw 'Runtime escaped static environment wrapper' }
Invoke-AirkTool Record-Child @('scripts/package-native.py')
if (($captured -join '|') -ne 'scripts/package-native.py') { throw 'Unrelated command changed' }
"""
        result = subprocess.run([shutil.which("pwsh"), "-NoProfile", "-Command", script], cwd=ROOT, text=True, capture_output=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
