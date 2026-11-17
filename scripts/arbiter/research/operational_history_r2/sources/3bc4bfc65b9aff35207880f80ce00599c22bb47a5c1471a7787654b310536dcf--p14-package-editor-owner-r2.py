# SPDX-License-Identifier: Apache-2.0
"""Single local P14 package/private-editor owner. No publication or native build.

Run only after measured campaigns finish. Uses the actual WORK quota-corrected producer,
then actual VSIX installation in private profiles. Stops after a failed stage;
keeps existing logs, receipts and owned fixture evidence. Never changes ACLs or
normal user editor/native installations.
"""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys

BASE = Path(__file__).resolve().parent
WORK = BASE / 'p10-worktree'
RUNTIME = BASE / 'p14-runtime-final-r2'
EXPECTED = {'sha256': '5d8912b117b410306ac9a3b44af6d96d70bb018006c2d49a1efa365597fe3e11', 'files': 960}
OUTPUT = WORK / 'artifacts/p14-package-editor-r2'
RECORD = BASE / 'p14-package-editor-owner-r2.json'
LEGACY = BASE / 'p14-legacy-upgrade-editor-owner-r2.py'
LEGACY_OWNER = WORK / 'artifacts/p14-legacy-upgrade-editor-owner-r2'
EDITOR = Path('C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/Code.exe')
CLI = Path('C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/a717861c07/resources/app/out/cli.js')


def read(path):
    return json.loads(Path(path).read_text(encoding='utf-8'))


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def extension_inputs():
    paths = []
    for folder in ['packages/vscode/src', 'packages/vscode/scripts', 'packages/vscode/images']:
        paths.extend(path for path in (WORK / folder).rglob('*') if path.is_file())
    paths.extend((WORK / 'packages/vscode').glob('tsconfig*.json'))
    paths.extend(WORK / name for name in ['packages/vscode/package.json',
                 'packages/vscode/package-lock.json', 'packages/vscode/README.md',
                 'package.json', 'package-lock.json'])
    return {path.relative_to(WORK).as_posix(): sha(path) for path in sorted(set(paths))}


def main():
    assert not RECORD.exists() and not OUTPUT.exists() and not LEGACY_OWNER.exists(), 'Fresh owner/output required'
    assert EDITOR.is_file() and CLI.is_file()
    prepared = read(BASE / 'p14-package-editor-owner-r2-prepared.json')
    assert sha(Path(__file__)) == prepared['owner_sha256'], 'Reviewed owner changed'
    assert all(sha(Path(name)) == digest for name, digest in prepared['pins'].items()), 'Reviewed input changed'
    assert sha(LEGACY) == 'c4e8d7d7d0df99499533ef0ea0df0031d3f6b7062482521cf3f6be50e0c4c093', 'Reviewed legacy clone changed'
    os.environ.update(PYTHONUTF8='1', CARGO_NET_OFFLINE='true',
        AIRK_PRIVATE_CENSUS_BASE=str(BASE / 'p10-private-fixtures-619c272e8a644988b26bbe4334fc8c02'),
        GIT_CONFIG_GLOBAL=str(BASE / 'p10-owned-ignore-controls/global.gitconfig'))
    builder = load('p14_editor_builder', WORK / 'scripts/build-service-runtime.py')
    sealer = load('p14_editor_sealer', BASE / 'p9-integrated-checks.py')
    assert builder.source_identity() == EXPECTED
    release = read(BASE / 'p14-release-runtime-r2.receipt.json')
    assert release['exit_code'] == release['actual_exit_code'] == 0
    assert release['source_before'] == release['source_after'] == EXPECTED and release['source_unchanged'] is True
    assert sha(BASE / 'p14-release-runtime-r2.log') == release['log_sha256']
    producer_sha = sha(BASE / 'p14-release-runtime-r2.receipt.json')
    seal = read(BASE / 'p14-runtime-final-r2-seal.json')
    assert sealer.seal_runtime(RUNTIME, EXPECTED) == seal
    ui = extension_inputs()
    scripts = {name: sha(WORK / name) for name in [
        'scripts/package-native.py', 'scripts/package-extension.py', 'scripts/check-editor-windows.cjs',
        'scripts/check-native-editor.cjs', 'scripts/editor-ui.cjs',
    ]}
    record = {'status': 'running', 'native_source': EXPECTED, 'producer_receipt_sha256': producer_sha, 'runtime_seal_sha256': sha(BASE / 'p14-runtime-final-r2-seal.json'),
              'extension_input_hashes': ui, 'driver_hashes': scripts, 'steps': [],
              'limits': 'Local unsigned package and selected private installed-editor scenarios; no publication, normal-install change, signing/notarization, Mac/Linux editor or final post-recovery user installer claim.'}

    def save():
        RECORD.write_text(json.dumps(record, indent=2) + '\n', encoding='utf-8')

    def invariant():
        assert builder.source_identity() == EXPECTED
        assert sha(BASE / 'p14-release-runtime-r2.receipt.json') == producer_sha
        assert sha(BASE / 'p14-release-runtime-r2.log') == release['log_sha256']
        assert extension_inputs() == ui, 'Extension source/listing/assets changed'
        assert all(sha(WORK / name) == digest for name, digest in scripts.items())
        assert sealer.seal_runtime(RUNTIME, EXPECTED) == seal
        if 'vsix' in record:
            assert sha(Path(record['vsix']['path'])) == record['vsix']['sha256'], 'Actual VSIX changed'

    def run(name, command, extra=None):
        invariant()
        env = os.environ.copy()
        for key in ['AIRK_EDITOR_LEGACY_FIXTURE', 'AIRK_EDITOR_SCENARIOS', 'AIRK_TEST_BINARY',
                    'AIRK_CATALOG_CANDIDATE_BINARY', 'AIRK_P10_RESOURCE_OUTPUT_DIR', 'AIRK_P11B_RESOURCE_OUTPUT_DIR']:
            env.pop(key, None)
        env.update(extra or {})
        label = 'p14-package-' + name + '-r2.log'
        assert not (BASE / label).exists(), 'Never overwrite a preceding stage'
        result = subprocess.run([sys.executable, '-X', 'utf8', str(BASE / 'p11-local-command.py'), label, *command], cwd=WORK, env=env)
        path = BASE / label.replace('.log', '.receipt.json')
        value = read(path) if path.exists() else None
        record['steps'].append({'name': name, 'exit_code': result.returncode, 'receipt': str(path),
                                'receipt_sha256': sha(path) if path.exists() else None})
        save()
        assert result.returncode == 0 and value is not None, name
        assert value['command'] == command and value['exit_code'] == value['actual_exit_code'] == 0
        assert value['source_before'] == value['source_after'] == EXPECTED and value['source_unchanged'] is True
        assert sha(BASE / label) == value['log_sha256']
        invariant()

    def installed(name, scenarios, extra=None):
        base = WORK / 'artifacts/editor-qualification'
        before = set(base.iterdir()) if base.exists() else set()
        run(name, ['node', str(WORK / 'scripts/check-editor-windows.cjs'), str(EDITOR), str(CLI), record['vsix']['path']],
            {'AIRK_EDITOR_SCENARIOS': ','.join(scenarios), **(extra or {})})
        created = set(base.iterdir()) - before
        assert len(created) == 1, 'Expected one exact private editor owner'
        summary_path = created.pop() / 'summary.json'
        summary = read(summary_path)
        assert summary['status'] == 'passed' and summary['qualifiesFullScenarioSelection'] is True
        assert summary['selectedScenarios'] == scenarios and summary['staged'] is False
        assert Path(summary['source']).resolve() == Path(record['vsix']['path']).resolve()
        assert summary['nativeInstallDigest'] == record['native_install_sha256']
        assert {row['scenario'] for row in summary['scenarios']} == set(scenarios)
        assert len(summary['scenarios']) == sum(2 if name in ['same', 'different'] else 1 for name in scenarios)
        assert all(row['installed'] is True and row['version'] == '1.8.8' for row in summary['scenarios'])
        assert summary['legacyUpgradeCoverage'] is ('legacy' in scenarios)
        record.setdefault('installed', []).append({'scenarios': scenarios, 'summary': str(summary_path), 'sha256': sha(summary_path),
                                                  'vsix_sha256': record['vsix']['sha256']})
        save()

    save()
    try:
        run('native', ['python', 'scripts/package-native.py', '--target', 'x86_64-pc-windows-msvc',
                      '--runtime-dir', str(RUNTIME), '--output', str(OUTPUT / 'native')])
        native = read(OUTPUT / 'native/publish/manifest.json')
        assert native['source'] == EXPECTED and native['signed_distribution'] is False
        record['native_install_sha256'] = sha(RUNTIME / 'airk-install.json')
        assert native['install_manifest_sha256'] == record['native_install_sha256']
        # One canonical current-extension compile; no Rust/native rebuild.
        run('extension-compile', ['npm.cmd', 'run', 'compile', '--prefix', 'packages/vscode'])
        run('vsix', ['python', 'scripts/package-extension.py', '--target', 'x86_64-pc-windows-msvc',
                    '--native-dir', str(OUTPUT / 'native/publish'), '--output', str(OUTPUT / 'extension')])
        extension = read(OUTPUT / 'extension/publish/manifest.json')
        vsix = OUTPUT / 'extension/publish' / extension['archive']
        assert vsix.name == 'airk-1.8.8-win32-x64.vsix' and sha(vsix) == extension['archive_sha256']
        assert extension['native_manifest_sha256'] == sha(OUTPUT / 'native/publish/manifest.json')
        record['vsix'] = {'path': str(vsix), 'sha256': sha(vsix), 'manifest_sha256': sha(OUTPUT / 'extension/publish/manifest.json')}
        save()
        installed('five-private-scenarios', ['same', 'different', 'multi', 'usage', 'untrusted'])
        run('legacy-fixture', ['python', str(LEGACY)])
        fixture = read(LEGACY_OWNER / 'fixture.json')
        assert fixture['candidate']['install_manifest_sha256'] == record['native_install_sha256']
        assert fixture['source_hashes_before'] == fixture['source_hashes_after']
        record['legacy_fixture'] = {'path': str(LEGACY_OWNER / 'fixture.json'), 'sha256': sha(LEGACY_OWNER / 'fixture.json'),
                                    'clone_sha256': sha(LEGACY), 'vsix_sha256': record['vsix']['sha256']}
        save()
        installed('legacy-private-scenario', ['legacy'], {'AIRK_EDITOR_LEGACY_FIXTURE': str(LEGACY_OWNER / 'fixture.json')})
        invariant()
        record['status'] = 'passed'
        save()
    except BaseException as error:
        record['status'] = 'failed'
        record['failure'] = str(error)
        save()
        raise


if __name__ == '__main__':
    main()
