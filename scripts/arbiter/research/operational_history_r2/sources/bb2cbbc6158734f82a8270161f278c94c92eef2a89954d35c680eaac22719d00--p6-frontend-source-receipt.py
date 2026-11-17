# SPDX-License-Identifier: Apache-2.0
"""Scoped frontend source checks; no assets, native builds or installed qualification."""
from pathlib import Path
import datetime
import hashlib
import json
import re
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
NODE = ROOT / 'artifacts/tools/node-v24.15.0-win-x64/node.exe'
DESTINATION = ROOT / 'artifacts/advanced-split'
PREFIX = 'p6-frontend-source-frozen-20261114'
extensions = {'.ts', '.tsx', '.mjs', '.css', '.json'}
paths = {path for directory in ('packages/admin/src', 'packages/admin/test', 'packages/ui/src', 'packages/ui/test')
         for path in (ROOT / directory).rglob('*') if path.is_file() and path.suffix in extensions}
paths.update(ROOT / path for path in (
    'packages/vscode/src/adminUiContract.ts', 'packages/vscode/src/adminUiHost.ts',
    'packages/vscode/src/airkctl/session.ts', 'packages/vscode/src/airkctl/adminChunks.ts',
    'packages/vscode/src/airkctl/features/explorerHost.ts', 'packages/vscode/src/airkctl/features/liveStatus.ts',
    'packages/vscode/src/explorerContracts.ts', 'packages/vscode/src/explorerController.ts',
    'packages/vscode/src/explorerEvidence.ts', 'packages/vscode/src/explorerManagement.ts',
    'packages/vscode/src/workbenchContracts.ts', 'packages/vscode/src/workbenchView.ts',
    'packages/vscode/src/workbenchRender.ts', 'packages/vscode/scripts/build-explorer.cjs',
    'packages/vscode/src/generated/i2.ts', 'packages/vscode/src/generated/i3.d.ts', 'packages/vscode/src/generated/arbiter.d.ts',
    'packages/contracts-gen/i3.ts', 'packages/contracts-gen/arbiter.ts',
    'scripts/dev/contract_types.py', 'scripts/tests/test_contract_types.py',
    'scripts/package-extension.py', 'scripts/tests/test_package_extension.py',
    'fixtures/split/native_admin/chunks.json', 'fixtures/split/i3/mutation_preparation.json',
    'fixtures/split/ui_pilot/native_reads.json'))
paths.update(path for directory in ('packages/ui/generated', 'packages/vscode/src/explorerHelp') for path in (ROOT / directory).rglob('*') if path.is_file())
paths.add(ROOT / 'crates/airk-mcp/src/help/content.json')
for directory in ('packages/vscode/src/explorerWebview', 'packages/vscode/src/workbenchWebview'):
    paths.update(path for path in (ROOT / directory).rglob('*') if path.is_file() and path.suffix in extensions)

def hashes():
    return {path.relative_to(ROOT).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted(paths)}

admin_tests = sorted(str(path.relative_to(ROOT)) for path in (ROOT / 'packages/admin/test').glob('*.test.mjs') if '.dom.' not in path.name)
ui_tests = sorted(str(path.relative_to(ROOT)) for path in (ROOT / 'packages/ui/test').glob('*.test.mjs')
                  if '.dom.' not in path.name)
host_tests = [f'packages/vscode/out/test/{name}.test.js' for name in (
    'adminUiController', 'airkctlSession', 'workbench', 'workbenchDisplay', 'workbenchClientLifetime', 'workbenchOpenSelection',
    'workbenchFormDraftHost', 'workbenchMcpOptimizations', 'workbenchReferenceRefreshHost', 'explorerBoundaries', 'explorerLiveStatus')]
commands = [
    ('admin-types', [str(NODE), 'packages/admin/node_modules/typescript/bin/tsc', '-p', 'packages/admin/tsconfig.json']),
    ('editor-host-types', [str(NODE), 'packages/vscode/node_modules/typescript/bin/tsc', '-p', 'packages/vscode']),
    ('editor-webview-types', [str(NODE), 'packages/vscode/node_modules/typescript/bin/tsc', '-p', 'packages/vscode/tsconfig.webview.json']),
    ('native-type-generator', [sys.executable, '-m', 'unittest', 'scripts.tests.test_contract_types', '-v']),
    *[(f'admin-{Path(path).stem}', [str(NODE), '--test', path.replace('\\', '/')]) for path in admin_tests],
    *[(f'shared-{Path(path).stem}', [str(NODE), '--test', path.replace('\\', '/')]) for path in ui_tests],
    *[(f'host-{Path(path).stem}', [str(NODE), '--test', path]) for path in host_tests],
    ('packager-source-cases', [sys.executable, '-m', 'unittest', '-v', *[
        'scripts.tests.test_package_extension.VsixContracts.' + name for name in (
            'test_runtime_import_closure_includes_controls_without_test_extras',
            'test_ui_pilot_runtime_missing_refuses_before_any_staging',
            'test_bootstrap_runtime_uses_owning_node_verifier_on_exact_captured_bytes',
            'test_bootstrap_release_uses_native_binding_owner_on_exact_captured_bytes',
            'test_vsix_model_notice_owner_retains_exact_public_companions_and_rejects_extra_files',
            'test_runtime_manifest_refuses_unknown_externals_and_parser_drift')]]),
]
report = {'scope': 'Frontend source behavior/typechecks only; no native/installed/artifact qualification.',
          'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'before_sha256': hashes(), 'commands': [],
          'excluded': ['Real DOM retained in separate runs.', 'Final Help/native content and asset verification await native/version source freeze.',
                       'Actual VSIX staging requires the qualified native publication capture; no positive package claim here.']}
for name, command in commands:
    started = time.perf_counter()
    try:
        result = subprocess.run(command, cwd=ROOT, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
    except subprocess.TimeoutExpired as error:
        result = subprocess.CompletedProcess(command, 124, error.stdout or b'', error.stderr or b'')
    output = result.stdout.decode('utf-8', 'replace') + result.stderr.decode('utf-8', 'replace')
    log = DESTINATION / f'{PREFIX}-{name}.log'
    log.write_text(output, encoding='utf-8', newline='\n')
    selected = re.search(r'(?:ℹ tests|# tests) (\d+)', output) or re.search(r'Ran (\d+) tests?', output)
    count = int(selected[1]) if selected else None
    passed = result.returncode == 0 and (count is None if name.endswith('types') else count is not None and count > 0)
    record = {'name': name, 'argv': command, 'cwd': str(ROOT), 'elapsed_seconds': round(time.perf_counter() - started, 4),
              'exit_code': result.returncode, 'selected_count': count, 'passed': passed, 'log': log.relative_to(ROOT).as_posix()}
    report['commands'].append(record)
    (DESTINATION / f'{PREFIX}.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8', newline='\n')
    print(f'{name}: {"PASS" if passed else "FAIL"}, selected={count}, {record["elapsed_seconds"]}s', flush=True)
    if not passed:
        break
report['after_sha256'] = hashes()
report['changed_inputs'] = [path for path, digest in report['before_sha256'].items() if report['after_sha256'].get(path) != digest]
report['passed'] = len(report['commands']) == len(commands) and all(row['passed'] for row in report['commands']) and not report['changed_inputs']
report['finished_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
(DESTINATION / f'{PREFIX}.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8', newline='\n')
raise SystemExit(0 if report['passed'] else 1)
