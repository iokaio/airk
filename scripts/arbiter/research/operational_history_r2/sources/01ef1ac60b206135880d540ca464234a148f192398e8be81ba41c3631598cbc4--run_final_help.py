# SPDX-License-Identifier: Apache-2.0
"""Plan by default; root alone executes intentional native Help/UI generation."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
import tomllib

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
NODE = ROOT/'artifacts/tools/node-v24.15.0-win-x64/node.exe'
NODE_SHA = '3331e1ffe19874215472217c5e94f5a0c6d8e18c4ac7111d3937aa0ad5e9b4a5'
NPM = NODE.parent/'node_modules/npm/bin/npm-cli.js'
CARGO = Path('C:/Users/tdotn/.cargo/bin/cargo.exe')
NEXTEST = ROOT/'artifacts/tools/nextest/cargo-nextest.exe'
TARGET = ROOT/'artifacts/v190/p10-target'
STATIC = ROOT/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json'
STATIC_SHA = '0edd26b2f85e2ca84b24f2bfd83855ffebb593697e247532684c10e749aef870'
GROUPS = ['native', 'harness', 'fixtures', 'frontend']
WRITES = ['AIRK_WRITE_CLI_HELP', 'AIRK_WRITE_CLI_REFERENCE', 'AIRK_WRITE_HELP_CONTENT',
          'AIRK_WRITE_UI_PILOT_HELP', 'AIRK_WRITE_UI_PILOT_CLI', 'AIRK_WRITE_UI_PILOT_I3']
spec = importlib.util.spec_from_file_location('help_receipt_owner', ROOT/'scripts/dev/receipt_runner.py')
owner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(owner)

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def inputs():
    groups = owner.observe(GROUPS)
    files = {name: sha for group in groups.values() for name, sha in group['files'].items()}
    for name in ['README.md', 'docs/cli-reference.md', 'docs/arbiter/user-guide.md']:
        files[name] = digest(ROOT/name) if (ROOT/name).is_file() else 'missing'
    files['private:runner'] = digest(Path(__file__))
    files['private:static-environment'] = digest(STATIC)
    files['private:node'] = digest(NODE)
    files['private:npm-cli'] = digest(NPM)
    files['private:nextest'] = digest(NEXTEST)
    return files

def seal(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()

def versions():
    native = tomllib.loads((ROOT/'Cargo.toml').read_text(encoding='utf-8'))['workspace']['package']['version']
    extension = json.loads((ROOT/'packages/vscode/package.json').read_bytes())['version']
    lock = json.loads((ROOT/'packages/vscode/package-lock.json').read_bytes())['version']
    if (native, extension, lock) != ('1.9.0',)*3:
        raise ValueError('Reviewed native/extension/lock version changed')
    return native

def stages(out):
    steps = []
    def add(name, command, *, write=None, allowed=(), test=False, selected=None):
        steps.append(dict(name=name, command=list(map(str, command)), write_variable=write,
                          allowed_changes=list(allowed), test=test, expected_selected=selected))
    add('scoped-clippy', [sys.executable, 'scripts/dev-check.py', '-p', 'airkctl', '-p', 'airk-mcp',
        '-p', 'airk-https', '--all-features', '--offline', '--incremental', 'on', '--target-dir', TARGET,
        '--cargo', CARGO])
    emitters = [
        ('cli-catalog', 'airkctl', 'help_catalog_matches_the_embedded_airk_mcp_catalog', WRITES[0], 'crates/airk-mcp/src/help/cli_commands.json'),
        ('cli-reference', 'airkctl', 'cli_reference_documents_every_public_command', WRITES[1], 'docs/cli-reference.md'),
        ('native-content', 'airk-mcp', 'help_content_manifest_matches_native_owners', WRITES[2], 'crates/airk-mcp/src/help/content.json'),
        ('native-help-cases', 'airk-mcp', 'ui_pilot_help_native_cases', WRITES[3], 'fixtures/split/ui_pilot/help_native.json'),
        ('i2-cases', 'airkctl', 'ui_pilot_cli_semantic_cases', WRITES[4], 'fixtures/split/ui_pilot/native_reads.json'),
        ('i3-cases', 'airk-https', 'ui_pilot_i3_semantic_cases', WRITES[5], 'fixtures/split/ui_pilot/native_reads.json'),
    ]
    for name, crate, selector, variable, output in emitters:
        add(name, [CARGO, 'test', '-p', crate, '--lib', selector, '--all-features', '--locked', '--timings'],
            write=variable, allowed=[re.escape(output)], test=True, selected=1)
    expression = '(package(airk-mcp) & (test(help::) | test(ui_pilot_help_native_cases) | test(public_help_) | test(tool_help_) | test(native_help_detail_))) | (package(airkctl) & (test(help_catalog_matches_the_embedded_airk_mcp_catalog) | test(cli_reference_documents_every_public_command) | test(ui_pilot_cli_semantic_cases))) | (package(airk-https) & test(ui_pilot_i3_semantic_cases))'
    add('native-help-drift', [CARGO, 'nextest', 'run', '--profile', 'ci', '-p', 'airk-mcp', '-p', 'airkctl',
        '-p', 'airk-https', '--all-features', '--locked', '--timings', '-E', expression], test=True)
    for package in ['vscode', 'admin']:
        add('npm-ci-'+package, [NODE, NPM, 'ci', '--ignore-scripts', '--no-audit', '--no-fund', '--prefix', 'packages/'+package])
    add('metadata-codec', [NODE, 'packages/vscode/scripts/build-metadata-codec.cjs'])
    add('host-typescript', [NODE, 'packages/vscode/node_modules/typescript/bin/tsc', '-p', 'packages/vscode'])
    add('shared-help-manifest', [NODE, 'packages/vscode/scripts/build-explorer.cjs', '--write-help-manifest'],
        allowed=[re.escape('packages/ui/generated/help-manifest.json')])
    add('extension-compile', [NODE, NPM, 'run', 'compile', '--prefix', 'packages/vscode'])
    add('extension-help-tests', [NODE, '--test', 'packages/vscode/out/test/explorerHelp.test.js'], test=True)
    add('shared-help-tests', [NODE, '--test', *['packages/ui/test/'+name+'.test.mjs' for name in
        ['helpManifest', 'helpSearch', 'helpRoute', 'helpPresentation', 'help.dom', 'generatedHelp.dom']],
        'packages/admin/test/helpSemantics.test.mjs'], test=True)
    add('static-help-tests', [sys.executable, '-m', 'unittest', 'scripts.tests.test_help_site', '-v'], test=True)
    add('admin-build', [NODE, NPM, 'run', 'build', '--prefix', 'packages/admin'],
        allowed=[r'crates/airk-cli/admin-assets/(?:app\.[a-f0-9]{8}\.(?:js|css)|manifest\.json|index\.html|launch\.html)'])
    add('admin-check', [NODE, NPM, 'run', 'check', '--prefix', 'packages/admin'])
    add('admin-seal', [sys.executable, 'scripts/build-admin-assets.py', '--skip-install', '--output', out/'admin-assets.json'])
    return steps

def inventory(directory):
    result = {}
    for path in sorted(directory.rglob('*')):
        if path.is_file():
            if path.is_symlink():
                raise ValueError('Linked generated artifact')
            result[path.relative_to(ROOT).as_posix()] = {'bytes': path.stat().st_size, 'sha256': digest(path)}
    return result

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--execute-reviewed', action='store_true')
    parser.add_argument('--expected-source-sha256')
    parser.add_argument('--out', type=Path, default=ROOT/'artifacts/advanced-split/receipts/final-help-r1')
    parser.add_argument('--bundled-model-dir', type=Path, default=ROOT/'artifacts/advanced-split/bundled-model-v2')
    args = parser.parse_args()
    versions()
    if digest(STATIC) != STATIC_SHA:
        raise ValueError('Previously verified r5 static environment identity changed')
    if digest(NODE) != NODE_SHA:
        raise ValueError('Pinned Node24.15.0 executable identity changed')
    out = args.out.resolve()
    if not out.is_relative_to(ROOT/'artifacts') or out == ROOT/'artifacts':
        raise ValueError('Receipt output must be private under artifacts')
    npm_cache = (ROOT/'artifacts/advanced-split/npm-cache').resolve()
    if not npm_cache.is_relative_to(ROOT.resolve()) or (npm_cache.exists() and not npm_cache.is_dir()):
        raise ValueError('npm cache must remain a directory inside the workspace')
    before = inputs()
    model_directory = str(args.bundled_model_dir.resolve())
    review_input = {'source_files': before, 'bundled_model_directory': model_directory}
    plan = {'schema': 'airk.private-final-help-plan/1', 'version': versions(), 'source_sha256': seal(review_input),
            'source_files': before, 'steps': stages(out), 'out': str(out), 'target': str(TARGET),
            'incremental': '1', 'node_sha256': digest(NODE), 'execute_reviewed': args.execute_reviewed}
    plan['bundled_model_directory'] = model_directory
    plan['npm_cache_directory'] = str(npm_cache)
    if not args.execute_reviewed:
        print(json.dumps(plan, indent=2)); return 0
    if args.expected_source_sha256 != plan['source_sha256']:
        raise ValueError('Root-reviewed source seal differs; do not run stale generation')
    if out.exists():
        raise ValueError('Use a fresh receipt output; failed evidence is never overwritten')
    out.mkdir(parents=True)
    (out/'plan.json').write_text(json.dumps(plan, indent=2)+'\n', encoding='utf-8')
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith('AIRK_WRITE_') and key.upper() not in ['NPM_CONFIG_CACHE', 'NPM_CONFIG_LOGS_DIR', 'NPM_CONFIG_UPDATE_NOTIFIER']}
    environment.update(json.loads(STATIC.read_bytes())['environment'])
    environment.update(CARGO_TARGET_DIR=str(TARGET), CARGO_INCREMENTAL='1', CARGO_BUILD_JOBS='8',
        CARGO_NET_OFFLINE='true', AIRK_BUNDLED_MODEL_DIR=plan['bundled_model_directory'],
        PATH=os.pathsep.join([str(NODE.parent), str(NEXTEST.parent), str(CARGO.parent), environment.get('PATH', '')]),
        PYTHONIOENCODING='utf-8', NPM_CONFIG_CACHE=plan['npm_cache_directory'],
        NPM_CONFIG_LOGS_DIR=str(npm_cache/'_logs'), NPM_CONFIG_UPDATE_NOTIFIER='false')
    usage = ROOT/'fixtures/split/ui_pilot/usage_native.json'
    usage_sha = digest(usage)
    results = []
    expected_inputs = before
    for step in plan['steps']:
        initial = inputs(); child = dict(environment)
        if initial != expected_inputs:
            refusal = {'name': step['name'], 'result': 'refused_source_changed_between_steps',
                       'expected_sha256': seal(expected_inputs), 'observed_sha256': seal(initial),
                       'changed_inputs': sorted(name for name in initial.keys() | expected_inputs.keys()
                                                if initial.get(name) != expected_inputs.get(name)),
                       'command_executed': False}
            results.append(refusal)
            (out/(step['name']+'.json')).write_text(json.dumps(refusal, indent=2)+'\n', encoding='utf-8')
            (out/'results.json').write_text(json.dumps(results, indent=2)+'\n', encoding='utf-8')
            print(json.dumps(refusal), flush=True)
            return 3
        if step['write_variable']:
            child[step['write_variable']] = '1'
        started = time.monotonic(); unix = time.time()
        with (out/(step['name']+'.log')).open('x', encoding='utf-8') as log:
            process = subprocess.run(step['command'], cwd=ROOT, env=child, stdout=log, stderr=subprocess.STDOUT)
        final = inputs(); changed = sorted(name for name in initial.keys() | final.keys() if initial.get(name) != final.get(name))
        unexpected = [name for name in changed if not any(re.fullmatch(pattern, name) for pattern in step['allowed_changes'])]
        text = (out/(step['name']+'.log')).read_text(encoding='utf-8', errors='replace')
        count = owner.selected_tests(text)
        receipt = dict(step, exit_code=process.returncode, elapsed_seconds=time.monotonic()-started,
            started_unix=unix, changed_inputs=changed, unexpected_changes=unexpected,
            changed_identities={name: {'before': initial.get(name), 'after': final.get(name)} for name in changed},
            selected_tests=count, input_sha256=seal(initial), output_sha256=seal(final),
            usage_input_unchanged=digest(usage)==usage_sha, target_directory=str(TARGET), incremental='1')
        timing = TARGET/'cargo-timings/cargo-timing.html'
        if step['name'] in ['scoped-clippy', 'native-help-drift'] or step['write_variable']:
            if timing.is_file() and timing.stat().st_mtime >= unix:
                shutil.copy2(timing, out/(step['name']+'-cargo-timing.html'))
        junit = TARGET/'nextest/ci/junit.xml'
        if step['name'] == 'native-help-drift' and junit.is_file() and junit.stat().st_mtime >= unix:
            saved_junit = out/(step['name']+'-junit.xml')
            shutil.copy2(junit, saved_junit)
            receipt['junit'] = {'path': saved_junit.name, 'sha256': digest(saved_junit)}
        for name in changed:
            path = ROOT/name
            if name not in unexpected and path.is_file():
                saved = out/'generated'/step['name']/name
                saved.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, saved)
        results.append(receipt)
        (out/(step['name']+'.json')).write_text(json.dumps(receipt, indent=2)+'\n', encoding='utf-8')
        (out/'results.json').write_text(json.dumps(results, indent=2)+'\n', encoding='utf-8')
        print(json.dumps({key: receipt[key] for key in ['name','exit_code','elapsed_seconds','selected_tests','unexpected_changes']}), flush=True)
        if process.returncode or unexpected or not receipt['usage_input_unchanged'] or (step['test'] and not count) or (step['expected_selected'] is not None and count != step['expected_selected']):
            return process.returncode or 3
        expected_inputs = final
    final_inputs = inputs()
    if final_inputs != expected_inputs:
        (out/'final-source-refusal.json').write_text(json.dumps({'expected_sha256': seal(expected_inputs),
            'observed_sha256': seal(final_inputs), 'result': 'source_changed_before_final_seal'}, indent=2)+'\n', encoding='utf-8')
        return 3
    output = {'schema': 'airk.private-final-help-result/1', 'result': 'generated_and_selected_consumers_passed',
        'version': versions(), 'initial_source_sha256': plan['source_sha256'], 'final_source_sha256': seal(final_inputs),
        'final_source_files': final_inputs,
        'usage_native_sha256': usage_sha, 'steps': results, 'extension_out': inventory(ROOT/'packages/vscode/out'),
        'admin_assets': inventory(ROOT/'crates/airk-cli/admin-assets'), 'not_installed_or_published_qualification': True}
    (out/'result.json').write_text(json.dumps(output, indent=2)+'\n', encoding='utf-8')
    return 0

if __name__ == '__main__':
    raise SystemExit(main())
