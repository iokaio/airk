# SPDX-License-Identifier: Apache-2.0
"""Run one frozen P9 source or Windows runtime campaign in a new output directory.

No automatic retries, publication, global Git changes or normal installation.
Native source drift closes the campaign. Documentation-only changes are recorded
separately and do not change the compiled native source identity.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
ARTIFACTS = ROOT / 'artifacts/v190'
ANSI = re.compile(r'\x1b\[[0-?]*[ -/]*[@-~]')


def utc():
    return datetime.now(timezone.utc).isoformat()


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def write_receipt(output, report):
    temporary = output / 'receipt.json.pending'
    temporary.write_text(json.dumps(report, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')
    temporary.replace(output / 'receipt.json')


def git(*arguments):
    return subprocess.check_output(['git', '-c', f'safe.directory={ROOT.as_posix()}', *arguments], cwd=ROOT)


def public_source():
    excluded = {'.git', '.codex', '.agents', '.aws', '.mcp.json', 'temp', 'target', 'artifacts'}
    names = git('ls-files', '-z', '--cached', '--others', '--exclude-standard').split(b'\0')
    names = sorted({name for name in names if name and os.fsdecode(name).split('/')[0] not in excluded})
    value = hashlib.sha256()
    files, missing = 0, []
    for name in names:
        path = ROOT / os.fsdecode(name)
        if path.is_symlink() or not path.resolve().is_relative_to(ROOT):
            raise RuntimeError('Public source escaped the frozen repository')
        value.update(len(name).to_bytes(8, 'big'))
        value.update(name)
        if path.is_file():
            value.update(bytes.fromhex(digest(path)))
            files += 1
        else:
            value.update(b'missing')
            missing.append(os.fsdecode(name))
    return {'sha256': value.hexdigest(), 'files': files, 'missing_tracked_paths': missing}


def suite_summaries(path):
    text = ANSI.sub('', path.read_text(encoding='utf-8', errors='replace'))
    rust = [{'result': result, 'passed': int(passed), 'failed': int(failed),
             'ignored': int(ignored), 'measured': int(measured), 'filtered_out': int(filtered),
             'seconds': float(seconds)}
            for result, passed, failed, ignored, measured, filtered, seconds in re.findall(
                r'test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out; finished in ([0-9.]+)s', text)]
    node, current = [], None
    for line in text.splitlines():
        match = re.fullmatch(r'\s*[#ℹ]\s+(tests|suites|pass|fail|cancelled|skipped|todo|duration_ms)\s+([0-9.]+)\s*', line)
        if not match:
            continue
        key, number = match.groups()
        if key == 'tests':
            if current is not None:
                node.append(current)
            current = {}
        if current is not None:
            current[key] = float(number) if key == 'duration_ms' else int(number)
    if current is not None:
        node.append(current)
    python = []
    for match in re.finditer(r'Ran (\d+) tests? in ([0-9.]+)s\s*\n\s*(OK|FAILED)(?:\s*\(([^\n]*)\))?', text):
        ran, seconds, result, counters = match.groups()
        row = {'ran': int(ran), 'seconds': float(seconds), 'result': result,
               'skipped': 0, 'failures': 0, 'errors': 0}
        for key, number in re.findall(r'(skipped|failures|errors)=(\d+)', counters or ''):
            row[key] = int(number)
        if result == 'OK':
            row['passed'] = row['ran'] - row['skipped']
        python.append(row)
    rust_totals = None
    if rust:
        rust_totals = {'completed_suite_summaries': len(rust)}
        for key in ('passed', 'failed', 'ignored', 'measured', 'filtered_out'):
            rust_totals[key] = sum(row[key] for row in rust)
    return {'rust': rust, 'rust_completed_totals': rust_totals, 'node': node, 'python': python,
            'count_limit': 'Only actual completed log summaries are counted; a failed command may leave later suites unrun.',
            'counts_available': bool(rust or node or python)}


def npm_command():
    node = shutil.which('node')
    npm = shutil.which('npm') or shutil.which('npm.cmd')
    if not node or not npm:
        raise RuntimeError('Node/npm prerequisites are unavailable')
    for candidate in (Path(npm).parent / 'node_modules/npm/bin/npm-cli.js',
                      Path(node).parent / 'node_modules/npm/bin/npm-cli.js'):
        if candidate.is_file():
            return [node, str(candidate)]
    if os.name != 'nt':
        return [npm]
    raise RuntimeError('Cannot find the installed npm JS entry; no shell fallback')


def build_inputs(mapping_path, environment):
    """Select preserved offline inputs independently of cleaned compiler targets."""
    mapping_path = mapping_path.resolve()
    if mapping_path.is_file():
        mapping = json.loads(mapping_path.read_text(encoding='utf-8-sig'))
        models = Path(mapping['inputs']['local-vector-model']).resolve(strict=True)
        runtime = Path(mapping['inputs']['local-onnx-runtime']).resolve(strict=True)
        receipt = Path(mapping['receipt']).resolve(strict=True)
        observation = {'selection': 'preserved-input mapping', 'mapping': str(mapping_path),
                       'mapping_sha256': digest(mapping_path), 'clean_receipt': str(receipt),
                       'clean_receipt_sha256': digest(receipt)}
    else:
        models = Path(environment.get('AIRK_BUNDLED_MODEL_DIR', ROOT / 'target/local-vector-model')).resolve(strict=True)
        runtime = Path(environment.get('AIRK_BUNDLED_RUNTIME_DIR', ROOT / 'target/local-onnx-runtime')).resolve(strict=True)
        observation = {'selection': 'environment or documented default inputs',
                       'mapping_not_found': str(mapping_path)}
    test_model = models / 'minilm-l6-v2-u8-v1'
    assert models.is_dir() and runtime.is_dir() and (runtime / 'windows-x86_64').is_dir()
    assert (test_model / 'model.json').is_file(), 'Missing preserved x64 model inputs; do not rebuild against deleted target paths'
    environment.update(AIRK_BUNDLED_MODEL_DIR=str(models), AIRK_BUNDLED_RUNTIME_DIR=str(runtime),
                       AIRK_TEST_MODEL_DIR=str(test_model))
    observation.update(models=str(models), onnx_runtime=str(runtime), test_model=str(test_model),
                       test_model_manifest_sha256=digest(test_model / 'model.json'))
    return observation


def private_base(receipt_path, environment):
    record = json.loads(receipt_path.read_text(encoding='utf-8-sig'))
    assert record['schema'] == 'airk.p9.private-fixture-base/1'
    assert record['existing_acl_changed'] is False
    base = Path(record['path']).resolve(strict=True)
    assert base.is_dir() and base.parent == ARTIFACTS.resolve()
    assert base.name.startswith('p9-private-fixtures-')
    if os.name != 'nt':
        raise RuntimeError('This private-base receipt adapter is Windows-specific')
    environment['AIRK_P9_PRIVATE_BASE_CHECK'] = str(base)
    probe = r'''$ErrorActionPreference='Stop'; $sid=[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value; $acl=Get-Acl -LiteralPath $env:AIRK_P9_PRIVATE_BASE_CHECK; $rules=@($acl.GetAccessRules($true,$true,[System.Security.Principal.SecurityIdentifier])); [pscustomobject]@{current_sid=$sid;owner_sid=$acl.GetOwner([System.Security.Principal.SecurityIdentifier]).Value;protected=$acl.AreAccessRulesProtected;rules=@($rules | ForEach-Object {[pscustomobject]@{sid=$_.IdentityReference.Value;rights=[int]$_.FileSystemRights;inheritance=[int]$_.InheritanceFlags;propagation=[int]$_.PropagationFlags;access=[int]$_.AccessControlType;inherited=$_.IsInherited}});sddl=$acl.Sddl} | ConvertTo-Json -Depth 5 -Compress'''
    observed = json.loads(subprocess.check_output(['pwsh', '-NoProfile', '-Command', probe],
                                                 env=environment, text=True, encoding='utf-8'))
    assert observed['current_sid'] == observed['owner_sid'] == record['owner_sid']
    assert observed['protected'] is True and len(observed['rules']) == 1
    assert observed['rules'][0] == {'sid': record['owner_sid'], 'rights': 2032127,
                                   'inheritance': 3, 'propagation': 0, 'access': 0, 'inherited': False}
    assert observed['sddl'] == record['sddl']
    environment.pop('AIRK_P9_PRIVATE_BASE_CHECK')
    environment['AIRK_PRIVATE_CENSUS_BASE'] = str(base)
    return {'receipt_path': str(receipt_path), 'receipt_sha256': digest(receipt_path),
            'path': str(base), 'owner_sid': observed['owner_sid'], 'sddl': observed['sddl'],
            'same_token_and_native_dacl_verified': True, 'existing_acl_changed': False}


def seal_runtime(runtime, native_source):
    hold = ARTIFACTS / 'p9-retirement-review-hold.json'
    if hold.exists() and json.loads(hold.read_text())['status'] == 'hold':
        raise RuntimeError('P9 retirement review requires a correction before further runtime qualification')
    assert runtime.is_dir() and not runtime.is_symlink()
    files = {}
    for path in sorted(runtime.rglob('*')):
        assert not path.is_symlink() and not (hasattr(path, 'is_junction') and path.is_junction())
        if path.is_file():
            files[path.relative_to(runtime).as_posix()] = digest(path)
    install = json.loads((runtime / 'airk-install.json').read_text(encoding='utf-8'))
    metadata = json.loads((runtime / 'airk-runtime.json').read_text(encoding='utf-8'))
    build = json.loads((runtime / 'airk-build.json').read_text(encoding='utf-8'))
    assert set(files) == set(install['files']) | {'airk-install.json'}
    assert all(files[name] == expected for name, expected in install['files'].items())
    assert install['version'] == metadata['version'] == '1.8.8'
    assert build['source'] == native_source and build['profile'] == 'release'
    assert build['runtime'] == metadata
    assert files['airk.exe'] == metadata['airk_sha256']
    assert files['airk-embed.exe'] == metadata['embed_sha256']
    return {'path': str(runtime), 'files': files, 'runtime': metadata, 'build': build,
            'install_manifest_sha256': files['airk-install.json'], 'verified_inventory': True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--stage', choices=('source', 'runtime'), default='source')
    parser.add_argument('--private-base-receipt', type=Path, required=True)
    parser.add_argument('--inputs-mapping', type=Path, default=ARTIFACTS / 'p9-clean-inputs.local.json',
                        help='Preserved offline model/runtime inputs after cargo clean; fallback is inherited input env')
    parser.add_argument('--dependency-offline', action='store_true',
                        help='Explicit limited cached dependency audit; never qualifies a fresh release audit')
    args = parser.parse_args()
    if os.name != 'nt':
        parser.error('Windows integrated adapter; use the isolated Neo plan for macOS')
    output = args.output.resolve()
    assert output.is_relative_to(ARTIFACTS.resolve()) and output != ARTIFACTS.resolve()
    output.mkdir(parents=True, exist_ok=False)
    environment = os.environ.copy()
    cargo_home = Path(environment.get('CARGO_HOME', 'C:/Users/tdotn/.cargo'))
    environment['CARGO_HOME'] = str(cargo_home)
    environment['RUSTUP_HOME'] = environment.get('RUSTUP_HOME', 'C:/Users/tdotn/.rustup')
    environment['PATH'] = str(cargo_home / 'bin') + os.pathsep + environment.get('PATH', '')
    inputs = build_inputs(args.inputs_mapping, environment)
    environment['CARGO_INCREMENTAL'] = '0'
    environment['PYTHONUTF8'] = '1'
    environment['PYTHONDONTWRITEBYTECODE'] = '1'
    # Source fixtures use Cargo's just-built native binaries. Installed-runtime
    # extension/process fixtures are exercised only against the fresh pair below.
    inherited_test_binary = environment.pop('AIRK_TEST_BINARY', None)
    # Append one scoped exception without replacing unrelated inherited config.
    count = int(environment.get('GIT_CONFIG_COUNT', '0'))
    environment[f'GIT_CONFIG_KEY_{count}'] = 'safe.directory'
    environment[f'GIT_CONFIG_VALUE_{count}'] = ROOT.as_posix()
    environment['GIT_CONFIG_COUNT'] = str(count + 1)
    os.environ.update(environment)
    owner = private_base(args.private_base_receipt.resolve(strict=True), environment)
    os.environ['AIRK_PRIVATE_CENSUS_BASE'] = environment['AIRK_PRIVATE_CENSUS_BASE']
    spec = importlib.util.spec_from_file_location('p9_builder_identity', ROOT / 'scripts/build-service-runtime.py')
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    initial_native = builder.source_identity()
    initial_public = public_source()
    commit = git('rev-parse', 'HEAD').decode().strip()
    dirty = git('status', '--porcelain=v1', '-z').decode(errors='replace').split('\0')
    selected_env = {key: environment[key] for key in (
        'PATH', 'CARGO_HOME', 'RUSTUP_HOME', 'AIRK_BUNDLED_MODEL_DIR', 'AIRK_BUNDLED_RUNTIME_DIR',
        'AIRK_TEST_MODEL_DIR', 'AIRK_PRIVATE_CENSUS_BASE', 'PYTHONUTF8', 'PYTHONDONTWRITEBYTECODE',
        'GIT_CONFIG_COUNT')}
    for key in ('CARGO_TARGET_DIR', 'CARGO_BUILD_JOBS', 'CARGO_NET_OFFLINE', 'CARGO_INCREMENTAL', 'RUST_TEST_THREADS'):
        selected_env[key] = environment.get(key)
    report = {'schema': 'airk.p9.integrated-checks/1', 'stage': args.stage, 'phase': 'P9',
              'working_version': '1.8.8', 'started_utc': utc(), 'entry_commit': commit,
              'entry_dirty_status': [row for row in dirty if row], 'native_source': initial_native,
              'inherited_test_binary_not_used': inherited_test_binary,
              'public_source_initial': initial_public, 'environment': selected_env,
              'offline_build_inputs': inputs,
              'compiler_target': {'path': environment.get('CARGO_TARGET_DIR', str(ROOT / 'target')),
                                  'directory_existed_before': Path(environment.get('CARGO_TARGET_DIR', ROOT / 'target')).exists(),
                                  'incremental': '0',
                                  'cache_reuse_qualification': 'No reused-cache qualification inferred from directory existence.'},
              'platform': platform.platform(), 'logical_cpus': os.cpu_count(),
              'private_base': owner, 'steps': [], 'status': 'running',
              'limitations': ['Only actual completed suite summaries are counted.',
                              'Documentation-only changes are recorded separately from native source drift.',
                              'No installed editor/manual, cross-platform or publication claim is made.']}
    write_receipt(output, report)
    python = sys.executable
    runtime = output / 'runtime-release'
    if args.stage == 'source':
        audit = [python, 'scripts/dependency-audit.py', '--output', str(output / 'dependencies.json')]
        if args.dependency_offline:
            audit.append('--offline')
        steps = [
            ('fmt', ['cargo', 'fmt', '--all', '--check']),
            ('workspace-build', ['cargo', 'build', '--workspace', '--locked']),
            ('clippy', ['cargo', 'clippy', '--workspace', '--all-targets', '--all-features', '--locked', '--', '-D', 'warnings']),
            ('workspace-tests', ['cargo', 'test', '--workspace', '--all-features', '--locked']),
            ('extension-tests', [*npm_command(), 'test', '--prefix', 'packages/vscode']),
            ('python-tests', [python, '-m', 'unittest', 'discover', '-s', 'scripts/tests', '-v']),
            ('repository', ['pwsh', '-NoProfile', '-File', 'scripts/check-repo.ps1']),
            ('license-notices', [python, 'scripts/license-notices.py', '--output', str(output / 'notices'), '--release']),
            ('dependency-audit', audit),
            ('whitespace', ['git', '-c', f'safe.directory={ROOT.as_posix()}', 'diff', '--check']),
            ('versions', [python, 'scripts/release-version.py', 'check', '--expected', '1.8.8']),
        ]
    else:
        environment.pop('AIRK_TEST_BINARY', None)
        steps = [
            ('runtime-build', [python, 'scripts/build-service-runtime.py', '--profile', 'release', '--destination', str(runtime)]),
            ('service-clients', [python, 'scripts/check-service-clients.py', '--runtime-dir', str(runtime), '--files', '16', '--model', '--report', str(output / 'service-clients.json')]),
            ('ci-local', ['pwsh', '-NoProfile', '-File', 'scripts/ci-local.ps1', '-SkipBuild', '-RuntimeDir', str(runtime), '-Cores', '4', '-Repeat', '5']),
        ]
    runtime_seal = None
    for name, command in steps:
        before = builder.source_identity()
        public_before = public_source()
        step_commit = git('rev-parse', 'HEAD').decode().strip()
        if before != initial_native:
            report['steps'].append({'name': name, 'command': command, 'status': 'not_run',
                                    'reason': 'frozen_native_source_changed_before_step',
                                    'native_source_before': before})
            report['source_drift'] = True
            break
        if args.stage == 'runtime' and name != 'runtime-build' and runtime_seal is None:
            report['steps'].append({'name': name, 'command': command, 'status': 'not_run',
                                    'reason': 'fresh_release_runtime_not_built_and_sealed'})
            write_receipt(output, report)
            continue
        if runtime_seal is not None:
            try:
                assert seal_runtime(runtime, initial_native) == runtime_seal
            except Exception as error:
                report['steps'].append({'name': name, 'command': command, 'status': 'not_run',
                                        'reason': 'sealed_runtime_changed_before_step',
                                        'seal_error': str(error)[:2048]})
                runtime_seal = None
                write_receipt(output, report)
                continue
            environment['AIRK_TEST_BINARY'] = str(runtime / 'airk.exe')
        log = output / (name + '.log')
        ci_parent = ROOT / 'artifacts/ci-local'
        ci_before = set(ci_parent.iterdir()) if name == 'ci-local' and ci_parent.exists() else set()
        started, observed = time.monotonic(), utc()
        row = {'name': name, 'command': command, 'cwd': str(ROOT), 'started_utc': observed,
               'commit_before': step_commit,
               'log': str(log), 'native_source_before': before, 'public_source_before': public_before}
        try:
            with log.open('x', encoding='utf-8') as destination:
                completed = subprocess.run(command, cwd=ROOT, env=environment,
                                           stdout=destination, stderr=subprocess.STDOUT,
                                           creationflags=subprocess.CREATE_NO_WINDOW)
            row['exit_code'] = completed.returncode
            row['status'] = 'passed' if completed.returncode == 0 else 'failed'
        except Exception as error:
            row.update(status='failed', exit_code=None, exception_type=type(error).__name__,
                       exception=str(error)[:2048])
        row.update(seconds=round(time.monotonic() - started, 3), completed_utc=utc())
        if log.exists():
            row.update(log_sha256=digest(log), summaries=suite_summaries(log))
        after = builder.source_identity()
        row['native_source_after'] = after
        row['commit_after'] = git('rev-parse', 'HEAD').decode().strip()
        row['public_source_after'] = public_source()
        row['native_source_unchanged'] = after == before == initial_native
        if not row['native_source_unchanged']:
            row['status'] = 'failed'
            row['source_drift'] = True
            report['source_drift'] = True
        if name == 'runtime-build' and row.get('exit_code') == 0:
            try:
                runtime_seal = seal_runtime(runtime, initial_native)
                (output / 'runtime-seal.json').write_text(json.dumps(runtime_seal, indent=2) + '\n', encoding='utf-8')
                row['runtime_seal_sha256'] = digest(output / 'runtime-seal.json')
            except Exception as error:
                row.update(status='failed', seal_failure=type(error).__name__, seal_error=str(error)[:2048])
        elif runtime_seal is not None:
            try:
                assert seal_runtime(runtime, initial_native) == runtime_seal
                row['sealed_runtime_unchanged'] = True
            except Exception as error:
                row.update(status='failed', sealed_runtime_unchanged=False,
                           seal_failure=type(error).__name__, seal_error=str(error)[:2048])
                runtime_seal = None
        if name == 'ci-local' and ci_parent.exists():
            created = sorted(set(ci_parent.iterdir()) - ci_before)
            row['ci_local_outputs'] = []
            for directory in created:
                summary = directory / 'summary.json'
                child = {'path': str(directory), 'summary_available': summary.is_file()}
                if summary.is_file():
                    try:
                        records = json.loads(summary.read_text(encoding='utf-8-sig'))
                        records = records if isinstance(records, list) else [records]
                        child.update(summary_sha256=digest(summary), results=records,
                                     passed=sum(result['Passed'] is True for result in records),
                                     failed=sum(result['Passed'] is False for result in records),
                                     expected_cases=15)
                        if len(records) != 15 or not all(result['Passed'] is True for result in records):
                            row['status'] = 'failed'
                    except Exception as error:
                        child.update(summary_error=type(error).__name__, detail=str(error)[:2048])
                        row['status'] = 'failed'
                else:
                    row['status'] = 'failed'
                row['ci_local_outputs'].append(child)
            if len(created) != 1:
                row.update(status='failed', ci_local_output_ambiguous=True)
        for filename in ('dependencies.json', 'service-clients.json'):
            path = output / filename
            if (name == 'dependency-audit' and filename == 'dependencies.json'
                    or name == 'service-clients' and filename == 'service-clients.json') and path.is_file():
                try:
                    row['structured_report'] = json.loads(path.read_text(encoding='utf-8'))
                    row['structured_report_sha256'] = digest(path)
                except Exception as error:
                    row.update(status='failed', structured_report_error=type(error).__name__,
                               structured_report_detail=str(error)[:2048])
        report['steps'].append(row)
        write_receipt(output, report)
        print(json.dumps({'step': name, 'status': row['status'], 'exit_code': row.get('exit_code'),
                          'seconds': row['seconds'], 'receipt': str(output / 'receipt.json')}), flush=True)
        if report.get('source_drift'):
            break
    completed_names = {row['name'] for row in report['steps']}
    report['not_run_steps'] = [name for name, _ in steps if name not in completed_names]
    report['native_source_final'] = builder.source_identity()
    report['final_commit'] = git('rev-parse', 'HEAD').decode().strip()
    report['public_source_final'] = public_source()
    report['native_source_unchanged'] = report['native_source_final'] == initial_native
    report['public_source_unchanged'] = report['public_source_final'] == initial_public
    report['completed_utc'] = utc()
    report['status'] = ('passed' if len(report['steps']) == len(steps)
                        and all(row['status'] == 'passed' for row in report['steps'])
                        and report['native_source_unchanged'] and not report.get('source_drift') else 'failed')
    if args.dependency_offline:
        report['limitations'].append('Dependency audit explicitly offline; fresh npm/advisory coverage is unqualified.')
    write_receipt(output, report)
    print(json.dumps({'status': report['status'], 'stage': args.stage, 'receipt': str(output / 'receipt.json')}), flush=True)
    raise SystemExit(0 if report['status'] == 'passed' else 1)


if __name__ == '__main__':
    main()
