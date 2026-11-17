# SPDX-License-Identifier: Apache-2.0
"""One frozen-source Neo ARM clean-break component batch; no installed-release claim."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import shutil
import socket
import sys
import time
import tomllib
import xml.etree.ElementTree as ET

ROOT = Path('/Users/tylerjensen/airk-builds')
TOOLS = ROOT / 'timing-1.6.4.PutU1P'
STATIC_ENVIRONMENT = ROOT / 'one-airk-static-ort-1814-1794087059/static-environment-recovery.json'
NEXTTEST_SHA256 = 'aac4a0d9109efcfb4d83ec729c6e3204d5c15bf16ff554b5ee4476f6c5f60b37'
FILTER = 'test(clean_break) | test(rebuild) | test(retained_cleanup_source_roots) | test(lifecycle_watermark) | test(retry_successors)'
PACKAGES = ['airk-core', 'airk-store', 'airk-cli', 'airk-mcp']
REQUIRED_GATES = ['scoped-four-crate-clippy', 'focused-clean-break-native-tests', 'full-python-contracts',
                  'extension-locked-dependencies', 'extension-full-compile', 'full-mocked-extension-tests']


def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def load(source, name, filename):
    specification = importlib.util.spec_from_file_location(name, source / 'scripts' / filename)
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def require_junit(path):
    cases = list(ET.parse(path).iter('testcase'))
    executed = [case for case in cases if case.find('skipped') is None]
    if not executed or any(case.find('failure') is not None or case.find('error') is not None for case in cases):
        raise ValueError('Nextest must execute nonzero selected tests without failures or errors')
    return {'selected': len(cases), 'executed': len(executed), 'skipped': len(cases) - len(executed),
            'cases': [{'name': case.get('name'), 'classname': case.get('classname'),
                       'skipped': case.find('skipped') is not None} for case in cases]}


def require_python_count(log):
    import re
    rows = re.findall(r'^Ran (\d+) tests? in ', log.read_text(encoding='utf-8', errors='replace'), re.MULTILINE)
    if len(rows) != 1 or int(rows[0]) <= 0:
        raise ValueError('Python must report one nonzero selected-test count')
    return {'selected': int(rows[0]), 'summary': [line for line in log.read_text(encoding='utf-8', errors='replace').splitlines()
                                               if line.startswith(('Ran ', 'OK', 'FAILED'))]}


def require_node_count(log):
    import re
    text = log.read_text(encoding='utf-8', errors='replace')
    values = {}
    for name in ('tests', 'pass', 'fail', 'skipped', 'cancelled'):
        matches = re.findall(r'^(?:[\u2139#]\s+)?' + name + r'\s+(\d+)\s*$', text, re.MULTILINE)
        if len(matches) != 1:
            raise ValueError('Node omitted its complete test summary: ' + name)
        values[name] = int(matches[0])
    if values['tests'] <= 0 or values['pass'] <= 0 or values['fail'] or values['cancelled']:
        raise ValueError('Mocked extension suite must execute nonzero tests without failure/cancellation')
    return values


def finalize_qualification(state):
    """Reconcile the final command, including cleanup, before qualifying this scope."""
    rows = state.get('commands', [])
    settled = all(row.get('owned_command_started') is False or row.get('owned_descendants_settled') is True for row in rows)
    gates = all(sum(row.get('stage') == stage and row.get('result') == 'passed' for row in rows) == 1
                for stage in REQUIRED_GATES)
    state['all_owned_command_descendants_settled'] = settled
    state['qualification_complete'] = bool(state.get('result') == 'passed' and gates and settled
        and not state.get('unavailable') and state.get('evidence_preserved_before_clean') is True
        and state.get('cargo_cleanup') in ('completed', 'no-owned-compiler-target-created'))
    if state.get('result') == 'passed' and not state['qualification_complete']:
        state['result'] = 'failed'
    return state


def record_entry_failure(argv, error):
    """Bounded startup evidence; never claim ownership of or clean an existing target."""
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument('--campaign', type=Path)
    parser.add_argument('--native-sha256')
    args, _remaining = parser.parse_known_args(argv)
    if args.campaign is None or not hasattr(os, 'getuid') or os.getuid() != 501:
        raise ValueError('No authenticated Neo campaign for startup evidence')
    supplied = args.campaign.absolute()
    campaign = supplied.resolve(strict=True)
    if supplied != campaign or any(path.is_symlink() for path in (supplied, *supplied.parents)):
        raise ValueError('Startup evidence cannot follow redirected campaign paths')
    if campaign.parent != ROOT.resolve(strict=True) or not re.fullmatch(r'airk-clean-break-r32-final-[0-9]+', campaign.name):
        raise ValueError('Startup evidence is outside this isolated campaign')
    restored_path = campaign / 'restore-receipt.json'
    if restored_path.is_symlink() or not restored_path.is_file() or restored_path.stat().st_size > 4 * 1024 * 1024:
        raise ValueError('Startup evidence lacks its bounded restore receipt')
    restored = json.loads(restored_path.read_text(encoding='utf-8'))
    if restored.get('status') != 'RESTORED_FROZEN_PUBLIC_SOURCE' or restored.get('native_source', {}).get('sha256') != args.native_sha256:
        raise ValueError('Startup evidence crossed its restored source identity')
    evidence = campaign / 'evidence'
    if evidence.is_symlink():
        raise ValueError('Startup evidence directory redirected')
    evidence.mkdir(mode=0o700, exist_ok=True)
    raw = str(error).encode('utf-8', errors='replace')
    failure = {'schema': 1, 'startup_failure': True, 'result': 'failed', 'qualification_complete': False,
               'qualification_scope': 'scoped frozen ARM components only', 'campaign': str(campaign),
               'native_source': restored['native_source'], 'commands': [],
               'error': {'type': type(error).__name__, 'message': raw[:4096].decode('utf-8', errors='ignore'),
                         'original_bytes': len(raw), 'truncated': len(raw) > 4096},
               'cargo_cleanup': 'blocked_startup_target_ownership_unproven' if (campaign / 'cargo-target').exists()
                                else 'no-owned-compiler-target-created',
               'all_owned_command_descendants_settled': False,
               'scope_note': 'Entry-level failure; consult any preserved canonical batch receipt. Descendant settlement is not inferred and no foreign target was cleaned.'}
    with (evidence / 'owner-entry-failure.json').open('x', encoding='utf-8') as stream:
        json.dump(failure, stream, indent=2)
        stream.write('\n')
    batch = evidence / 'batch-receipt.json'
    if not batch.exists():
        with batch.open('x', encoding='utf-8') as stream:
            json.dump(failure, stream, indent=2)
            stream.write('\n')
    # Any earlier batch receipt remains intact; this terminal phase and the
    # separate entry-failure receipt make the overall owner failure explicit.
    (campaign / 'phase.txt').write_text('complete-failed', encoding='utf-8')
    return failure


def entrypoint(argv=None):
    selected = list(sys.argv[1:] if argv is None else argv)
    try:
        return main(selected)
    except SystemExit as error:
        if error.code in (None, 0):
            raise
        failure = error
    except BaseException as error:
        failure = error
    try:
        record_entry_failure(selected, failure)
    except BaseException as evidence_error:
        print('Neo owner startup evidence unavailable: ' + str(evidence_error)[:1024], file=sys.stderr)
    print('Neo owner startup failed: ' + str(failure)[:4096], file=sys.stderr)
    return 1


def main(argv=None):
    if sys.flags.optimize:
        raise ValueError('Optimized Python would discard batch ownership assertions')
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--campaign', type=Path, required=True)
    parser.add_argument('--native-sha256', required=True)
    args = parser.parse_args(argv)
    if platform.system() != 'Darwin' or platform.machine() != 'arm64' or os.getuid() != 501 or socket.gethostname() != 'Tylers-MacBook-Neo.local':
        raise ValueError('This owner is only for the established UID501 Neo ARM host')
    import resource
    campaign = args.campaign.resolve(strict=True)
    if campaign.parent != ROOT or not campaign.name.startswith('airk-clean-break-r32-final-') or campaign.is_symlink():
        raise ValueError('Choose the isolated reviewed r32 campaign, never an existing checkout')
    source = (campaign / 'source').resolve(strict=True)
    target, fixtures, evidence = campaign / 'cargo-target', campaign / 'fixtures', campaign / 'evidence'
    if target.exists() or target.is_symlink() or fixtures.exists() or evidence.exists():
        raise ValueError('Refusing to reuse another compiler/fixture/evidence owner')
    if (source / 'target').is_symlink():
        raise ValueError('Source fixture target must not redirect elsewhere')
    evidence.mkdir(mode=0o700)
    fixtures.mkdir(mode=0o700)
    started = time.monotonic()
    state = {'schema': 1, 'purpose': 'same frozen clean-break component tests on Neo ARM',
             'result': 'running', 'qualification_complete': False,
             'qualification_scope': 'scoped frozen ARM components only', 'host': socket.gethostname(),
             'uid': os.getuid(), 'architecture': platform.machine(), 'cpu_count': os.cpu_count(),
             'source': str(source), 'compiler_target': str(target), 'fixtures': str(fixtures),
             'incremental': 'off', 'cargo_build_jobs': 2, 'filter': FILTER, 'packages': PACKAGES,
             'native_configuration': {'default_features': False, 'features': ['local-inference'], 'clippy_targets': 'all-targets'},
             'commands': [], 'unavailable': [],
             'limits': ['No Mac installed clean-break/runtime/VSIX qualification from this component batch',
                        'No Intel, GUI Code/Aqua, signing/notarization, OS-trust, two-user or release claim']}
    owner = None
    development = None
    target_owned = False

    def save():
        state['elapsed_seconds'] = round(time.monotonic() - started, 3)
        (evidence / 'batch-receipt.json').write_text(json.dumps(state, indent=2) + '\n')

    def retain_timings(stage):
        destination = evidence / 'cargo-timings'
        destination.mkdir(mode=0o700, exist_ok=True)
        rows = []
        for path in sorted((target / 'cargo-timings').glob('*.html')):
            if path.is_symlink() or not path.is_file():
                raise ValueError('Cargo timing report is not a regular owned file')
            name = stage + '-latest.html' if path.name == 'cargo-timing.html' else path.name
            retained = destination / name
            if retained.exists():
                if sha(retained) != sha(path):
                    raise ValueError('Retained timing report would be overwritten')
            else:
                shutil.copy2(path, retained)
            rows.append({'path': str(retained), 'sha256': sha(retained), 'bytes': retained.stat().st_size})
        return rows

    try:
        manifest = json.loads((campaign / 'source-manifest.json').read_text())
        builder = load(source, 'r32_runtime_builder', 'build-service-runtime.py')
        owner = load(source, 'r32_owned_commands', 'provision-static-onnx-runtime.py')
        if manifest.get('status') != 'FROZEN_PUBLIC_SOURCE' or manifest.get('final_freeze_required') is not False:
            raise ValueError('Only a finalized reviewed public source package may run')
        state['source_identity'] = manifest['native_source']
        if manifest['native_source']['sha256'] != args.native_sha256:
            raise ValueError('Native source pin differs from restored package')
        state['source_roster_sha256'] = manifest['source_roster_sha256']
        state['source_manifest_sha256'] = sha(campaign / 'source-manifest.json')
        models_manifest = json.loads((campaign / 'public-model-transfer-manifest.json').read_text())
        state['public_models_roster_sha256'] = models_manifest['model_roster_sha256']
        if manifest['public_models_roster_sha256'] != state['public_models_roster_sha256']:
            raise ValueError('Restored public model roster differs from frozen source package')
        state['runner_sha256'] = sha(Path(__file__).resolve(strict=True))
        state['git_producer'] = manifest['head']

        def frozen_inputs():
            if sha(campaign / 'source-manifest.json') != state['source_manifest_sha256']:
                raise ValueError('Frozen source manifest changed')
            if builder.source_identity() != state['source_identity']:
                raise ValueError('Native inputs changed; stop the batch')
            for row in manifest['files']:
                relative = Path(row['path'])
                if relative.is_absolute() or '..' in relative.parts:
                    raise ValueError('Frozen source path escaped the checkout')
                path = source / relative
                if path.is_symlink() or not path.is_file() or path.stat().st_size != row['bytes'] or sha(path) != row['sha256']:
                    raise ValueError('Frozen public input changed: ' + row['path'])
            for row in models_manifest['files']:
                relative = Path(row['path'])
                if relative.is_absolute() or '..' in relative.parts:
                    raise ValueError('Frozen model path escaped the public model root')
                path = campaign / 'offline-model' / relative
                if path.is_symlink() or not path.is_file() or path.stat().st_size != row['bytes'] or sha(path) != row['sha256']:
                    raise ValueError('Frozen public model input changed: ' + row['path'])
            if sha(Path(__file__).resolve(strict=True)) != state['runner_sha256']:
                raise ValueError('Reviewed runner changed')
            return {'native': state['source_identity'], 'public_roster_sha256': state['source_roster_sha256']}

        frozen_inputs()
        git_context = {'GIT_DIR', 'GIT_WORK_TREE', 'GIT_INDEX_FILE', 'GIT_COMMON_DIR', 'GIT_OBJECT_DIRECTORY',
                       'GIT_ALTERNATE_OBJECT_DIRECTORIES', 'GIT_REPLACE_REF_BASE', 'GIT_NAMESPACE', 'GIT_CONFIG',
                       'GIT_CONFIG_COUNT', 'GIT_CONFIG_PARAMETERS'}
        python_context = {'PYTHONOPTIMIZE', 'PYTHONPATH', 'PYTHONHOME', 'PYTHONSTARTUP', 'PYTHONINSPECT'}
        base = {key: value for key, value in os.environ.items() if not key.startswith(('AIRK_', 'GIT_CONFIG_KEY_', 'GIT_CONFIG_VALUE_')) and key not in git_context and key not in python_context}
        state['inherited_python_redirect_keys_removed_from_child_only'] = sorted(key for key in os.environ if key in python_context)
        state['inherited_git_redirect_keys_removed_from_child_only'] = sorted(key for key in os.environ if key in git_context or key.startswith(('GIT_CONFIG_KEY_', 'GIT_CONFIG_VALUE_')))
        state['inherited_airk_keys_removed_from_child_only'] = sorted(key for key in os.environ if key.startswith('AIRK_'))
        base['PATH'] = ':'.join([str(Path.home() / '.cargo/bin'), str(TOOLS / 'tools/node-v24.15.0-darwin-arm64/bin'),
                                 str(TOOLS / 'tools'), str(Path(sys.executable).parent), base.get('PATH', '')])
        base.update(CARGO_TARGET_DIR=str(target), CARGO_BUILD_JOBS='2', CARGO_INCREMENTAL='0',
                    MACOSX_DEPLOYMENT_TARGET='13.3', AIRK_BUNDLED_MODEL_DIR=str(campaign / 'offline-model'),
                    AIRK_TEST_MODEL_DIR=str(campaign / 'offline-model/minilm-l6-v2-s8-v1'),
                    AIRK_PRIVATE_CENSUS_BASE=str(fixtures), TMPDIR=str(fixtures), TEMP=str(fixtures), TMP=str(fixtures),
                    PYTHONNOUSERSITE='1', PYTHONDONTWRITEBYTECODE='1')
        environment = load(source, 'r32_static_environment', 'static-runtime-environment.py').load_environment_file(
            STATIC_ENVIRONMENT, target='aarch64-apple-darwin')
        base.update(environment)
        state['static_environment'] = environment
        development = base
        nextest = TOOLS / 'tools/cargo-nextest'
        if not nextest.is_file() or nextest.is_symlink() or sha(nextest) != NEXTTEST_SHA256:
            raise ValueError('Pinned Nextest0.9.148 prerequisite unavailable or changed')
        if Path(shutil.which('cargo-nextest', path=base['PATH'])).resolve(strict=True) != nextest:
            raise ValueError('PATH selected a different Nextest')
        soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
        if hard < 4096:
            raise ValueError('Owned shell cannot supply required descriptor limit')
        resource.setrlimit(resource.RLIMIT_NOFILE, (4096, hard))
        state['nofile'] = {'before': [soft, hard], 'after': list(resource.getrlimit(resource.RLIMIT_NOFILE))}
        nextest_policy = tomllib.loads((source / '.config/nextest.toml').read_text())
        if 'store' in nextest_policy:
            raise ValueError('Source Nextest store policy requires fresh review')
        config = evidence / 'nextest.toml'
        config.write_bytes(('[store]\ndir = ' + json.dumps(str(evidence / 'nextest-store')) + '\n\n').encode()
                           + (source / '.config/nextest.toml').read_bytes())
        if tomllib.loads(config.read_text()) != {**nextest_policy, 'store': {'dir': str(evidence / 'nextest-store')}}:
            raise ValueError('Derived Nextest policy changed more than evidence storage')
        junit_relative = Path(nextest_policy['profile']['ci']['junit']['path'])
        if junit_relative.is_absolute() or '..' in junit_relative.parts:
            raise ValueError('Unsafe source JUnit path')
        junit = evidence / 'nextest-store/ci' / junit_relative
        state['nextest'] = {'version': '0.9.148', 'path': str(nextest), 'sha256': sha(nextest),
                            'source_policy_sha256': sha(source / '.config/nextest.toml'),
                            'config': str(config), 'config_sha256': sha(config), 'policy_changes': ['store.dir only'],
                            'user_config': 'none', 'junit': str(junit)}

        def run(name, command, timeout=1200, count=None, cleanup=False):
            nonlocal target_owned
            (campaign / 'phase.txt').write_text(name)
            row = {'stage': name, 'argv': [str(value) for value in command], 'scope': name, 'cwd': str(source),
                   'compiler_target': str(target), 'incremental': 'off', 'cargo_build_jobs': 2,
                   'cpu_count': state['cpu_count'], 'started_unix': time.time(), 'result': 'running',
                   'expected_result': 'exit zero; unchanged frozen inputs; nonzero counts for test selections',
                   'stop_condition': 'first failure, timeout, uncertainty, missing evidence or frozen-input change',
                   'owned_command_started': False}
            state['commands'].append(row)
            save()
            try:
                row['frozen_inputs_before'] = {'cleanup_only': True} if cleanup else frozen_inputs()
                if name == 'scoped-four-crate-clippy':
                    target_owned = True
                row['owned_command_started'] = True
                cpu_before = resource.getrusage(resource.RUSAGE_CHILDREN)
                result = owner.run_owned(row['argv'], source, evidence / (name + '.log'), timeout, development)
                row.update(result)
                cpu_after = resource.getrusage(resource.RUSAGE_CHILDREN)
                row['cpu_observation'] = {'scope': 'RUSAGE_CHILDREN difference observed by the batch owner',
                                           'user_seconds': cpu_after.ru_utime - cpu_before.ru_utime,
                                           'system_seconds': cpu_after.ru_stime - cpu_before.ru_stime}
                row['result'] = 'passed' if owner.successful(result) else 'failed'
                row['cargo_timings'] = retain_timings(name)
                if not cleanup:
                    row['frozen_inputs_after'] = frozen_inputs()
                if count == 'nextest':
                    if junit.is_file() and not junit.is_symlink():
                        cases = list(ET.parse(junit).iter('testcase'))
                        row['observed_test_counts'] = {'selected': len(cases),
                            'executed': sum(case.find('skipped') is None for case in cases),
                            'skipped': sum(case.find('skipped') is not None for case in cases),
                            'failures': sum(case.find('failure') is not None for case in cases),
                            'errors': sum(case.find('error') is not None for case in cases)}
                        row['junit'] = {'path': str(junit), 'bytes': junit.stat().st_size, 'sha256': sha(junit)}
                        if row['result'] == 'passed':
                            row['test_counts'] = require_junit(junit)
                    else:
                        row['observed_test_counts'] = {'selected': 0, 'executed': 0, 'reason': 'JUnit not produced; retain compiler/transport failure log'}
                        if row['result'] == 'passed':
                            raise ValueError('Successful Nextest omitted its required JUnit report')
                elif row['result'] == 'passed' and count == 'python':
                    row['test_counts'] = require_python_count(evidence / (name + '.log'))
                elif row['result'] == 'passed' and count == 'node':
                    row['test_counts'] = require_node_count(evidence / (name + '.log'))
                save()
                if row['result'] != 'passed':
                    raise RuntimeError(name + ' failed; collect before any correction')
            except BaseException as error:
                row['result'] = 'failed'
                row['error'] = str(error)
                save()
                raise

        package_arguments = [value for package in PACKAGES for value in ('-p', package)]
        run('scoped-four-crate-clippy', [sys.executable, source / 'scripts/dev-check.py', *package_arguments,
            '--features', 'local-inference', '--target-dir', target, '--incremental', 'off'], 3600)
        run('focused-clean-break-native-tests', ['cargo', 'nextest', 'run', *package_arguments,
            '--no-default-features', '--features', 'local-inference', '--profile', 'ci', '--config-file', config,
            '--user-config-file', 'none', '--test-threads', '2', '--locked', '--timings', '-E', FILTER], 3600, 'nextest')
        run('full-python-contracts', [sys.executable, '-m', 'unittest', 'discover', '-s', 'scripts/tests', '-v'], 1800, 'python')
        unavailable = [tool for tool in ('node', 'npm') if not shutil.which(tool, path=base['PATH'])]
        if unavailable:
            state['unavailable'].append({'component': 'mocked-extension', 'missing_tools': unavailable,
                                          'outcome': 'not run; not passing'})
        else:
            run('extension-locked-dependencies', ['npm', 'ci', '--prefix', 'packages/vscode', '--prefer-offline',
                '--ignore-scripts', '--no-audit', '--no-fund'], 600)
            run('extension-full-compile', ['npm', 'run', 'compile', '--prefix', 'packages/vscode'], 600)
            tests = sorted(source.glob('packages/vscode/out/test/*.test.js'))
            if not tests:
                raise ValueError('No mocked extension test inputs were compiled')
            run('full-mocked-extension-tests', ['node', '--test', '--test-concurrency=2', *tests], 1800, 'node')
        state['result'] = 'incomplete_unavailable' if state['unavailable'] else 'passed'
    except BaseException as error:
        state['result'] = 'failed'
        state['error'] = str(error)
    finally:
        save()
        try:
            state['retained_cargo_timings'] = retain_timings('final')
            state['evidence_preserved_before_clean'] = True
        except BaseException as error:
            state['evidence_preserved_before_clean'] = False
            state['evidence_preservation_error'] = str(error)
            state['result'] = 'failed'
        descendants_settled = all(row.get('owned_command_started') is False or row.get('owned_descendants_settled') is True
                                  for row in state['commands'])
        state['all_owned_command_descendants_settled'] = descendants_settled
        if target_owned and target.exists():
            if target.parent != campaign or target.name != 'cargo-target' or target.is_symlink() or target == source / 'target':
                state['cargo_cleanup'] = 'blocked_target_ownership'
                state['result'] = 'failed'
            elif owner is None or development is None or not descendants_settled or not state['evidence_preserved_before_clean']:
                state['cargo_cleanup'] = 'blocked_unsettled_or_evidence_unpreserved'
                state['result'] = 'failed'
            else:
                try:
                    run('required-neo-cargo-clean', ['cargo', 'clean', '--target-dir', target], 600, cleanup=True)
                    state['cargo_cleanup'] = 'completed'
                except BaseException as error:
                    state['cargo_cleanup'] = 'failed'
                    state['cleanup_error'] = str(error)
                    state['result'] = 'failed'
        else:
            state['cargo_cleanup'] = 'no-owned-compiler-target-created'
        state['compiler_tree_absent_or_empty'] = not target.exists() or not any(target.iterdir())
        state['source_and_fixtures_retained'] = source.is_dir() and fixtures.is_dir()
        finalize_qualification(state)
        save()
        (campaign / 'phase.txt').write_text('complete-' + state['result'])
        print(json.dumps({'result': state['result'], 'receipt': str(evidence / 'batch-receipt.json'),
                          'cargo_cleanup': state['cargo_cleanup']}), flush=True)
    return 0 if state['result'] == 'passed' else 1


if __name__ == '__main__':
    raise SystemExit(entrypoint())
