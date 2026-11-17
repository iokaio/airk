# SPDX-License-Identifier: Apache-2.0
"""Complete remaining components on the original frozen Neo r36 source; never run Cargo."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import shutil
import socket
import sys
import time
import xml.etree.ElementTree as ET

ROOT = Path('/Users/tylerjensen/airk-builds')
TOOLS = ROOT / 'timing-1.6.4.PutU1P/tools'
APPROVED = {
    'campaign': 'airk-clean-break-r35-final-1794218081236320400',
    'prior_sha256': 'b87968569da0f5d6c12fc7dd61eb84c5f8657b0c2a3cc4efc6552e050a40911f',
    'source_manifest_sha256': '74d76ce2e268f2620b2799aad9971d771aebe8c5f3fee1d02b2546d7cd2b7d08',
    'native': {'sha256': 'cf81a39ad2af229bd1fb8769c6f8badbc39057ca0cfb64acccd1c7b0939056c8', 'files': 1153},
    'source_roster_sha256': '914ea3ceda7f2bd4dd638105996f29c2efef53ae3b2eb7eb16e1eda77e993833',
    'models_roster_sha256': 'c8e33ba41ebb92f3cc4f65cf1e28e945cfa148a3dbb30b01e99e6e69e0d7868e',
    'source_file_count': 2142,
    'original_runner_sha256': '542174b012e14077fab1c863cca8dde0c32eb1eabf04d5d1850b333fc981e4d2',
    'native_selected': 393,
    'python_selected': 919,
    'logs': {
        'scoped-four-crate-clippy.log': 'f698cd65efd217613bb9239cfbff6dffcee39f9e6e97efdbee48ff2f289ad41f',
        'focused-clean-break-native-tests.log': '5e8f127cd47a5235e17c3fa03886439945b5bfd66b0e86083f0d0838ddb7d34f',
        'full-python-contracts.log': '50bdd9c4dc629e8af7da9e26c55c5e38680e9417b1fec6ac1cba330418501a3f',
        'required-neo-cargo-clean.log': '865dc53112ab08a3babf0cbece61d4668a481b9e827bb7320a855af8a49d3b80',
    },
}
REQUIRED_GATES = ['prepare-owned-artifacts', 'extension-locked-dependencies', 'extension-full-compile',
                  'full-python-contracts', 'full-mocked-extension-tests']


def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':')).encode()


def plain(path, directory=None):
    path = Path(path).absolute()
    if any(item.is_symlink() for item in (path, *path.parents)):
        raise ValueError('Redirected owned path: ' + str(path))
    resolved = path.resolve(strict=True)
    if resolved != path:
        raise ValueError('Noncanonical owned path: ' + str(path))
    if directory is True and not path.is_dir() or directory is False and not path.is_file():
        raise ValueError('Unexpected owned path kind: ' + str(path))
    return path


def read_json(path, maximum=4 * 1024 * 1024):
    path = plain(path, False)
    if path.stat().st_size > maximum:
        raise ValueError('Oversized metadata: ' + str(path))
    return json.loads(path.read_text(encoding='utf-8'))


def relative(root, name):
    if not isinstance(name, str) or not name or '\\' in name or ':' in name:
        raise ValueError('Invalid frozen relative path')
    parts = name.split('/')
    if PurePosixPath(name).is_absolute() or any(part in ('', '.', '..') for part in parts):
        raise ValueError('Frozen path escaped its namespace')
    path = plain(root.joinpath(*parts), False)
    if not path.is_relative_to(root):
        raise ValueError('Crossed frozen root')
    return path


def verify_rows(root, rows, expected_roster):
    if not isinstance(rows, list) or not rows or len(rows) > 10000:
        raise ValueError('Missing or excessive frozen roster')
    if hashlib.sha256(canonical(rows)).hexdigest() != expected_roster:
        raise ValueError('Frozen roster identity changed')
    seen = set()
    for row in rows:
        if row['path'] in seen:
            raise ValueError('Duplicate frozen input')
        seen.add(row['path'])
        path = relative(root, row['path'])
        if path.stat().st_size != row['bytes'] or sha(path) != row['sha256']:
            raise ValueError('Frozen input changed: ' + row['path'])


def validate_prior(campaign, approved=APPROVED):
    """Authenticate original failed batch; retain its successful native gates as historical facts."""
    campaign = plain(campaign, True)
    if campaign.name != approved['campaign']:
        raise ValueError('Crossed original campaign namespace')
    receipt_path = campaign / 'evidence/batch-receipt.json'
    receipt = read_json(receipt_path)
    if sha(receipt_path) != approved['prior_sha256']:
        raise ValueError('Original batch receipt identity changed')
    if (receipt.get('schema') != 1 or receipt.get('result') != 'failed'
        or receipt.get('qualification_complete') is not False
        or receipt.get('source') != str(campaign / 'source')
        or receipt.get('compiler_target') != str(campaign / 'cargo-target')
        or receipt.get('fixtures') != str(campaign / 'fixtures')
        or receipt.get('source_identity') != approved['native']
        or receipt.get('source_roster_sha256') != approved['source_roster_sha256']
        or receipt.get('public_models_roster_sha256') != approved['models_roster_sha256']
        or receipt.get('source_manifest_sha256') != approved['source_manifest_sha256']
        or receipt.get('runner_sha256') != approved['original_runner_sha256']
        or receipt.get('cargo_cleanup') != 'completed'
        or receipt.get('evidence_preserved_before_clean') is not True
        or receipt.get('all_owned_command_descendants_settled') is not True
        or receipt.get('compiler_tree_absent_or_empty') is not True):
        raise ValueError('Original native qualification/cleanup scope is not proven')
    expected_stages = ['scoped-four-crate-clippy', 'focused-clean-break-native-tests',
                       'full-python-contracts', 'required-neo-cargo-clean']
    rows = receipt.get('commands', [])
    if [row.get('stage') for row in rows] != expected_stages:
        raise ValueError('Original stage roster changed')
    for row in rows:
        passed = row['stage'] != 'full-python-contracts'
        if (row.get('result') != ('passed' if passed else 'failed')
            or row.get('exit_code') != (0 if passed else 1)
            or row.get('owned_command_started') is not True
            or row.get('owned_descendants_settled') is not True
            or row.get('timed_out') is not False or row.get('log_oversized') is not False):
            raise ValueError('Original stage is failed, uncertain or crossed')
        if row['stage'] != 'required-neo-cargo-clean':
            expected = {'native': approved['native'], 'public_roster_sha256': approved['source_roster_sha256']}
            if row.get('frozen_inputs_before') != expected or row.get('frozen_inputs_after') != expected:
                raise ValueError('Original stage consumed changed frozen inputs')
    native = rows[1]
    count = approved['native_selected']
    if native.get('observed_test_counts') != {'selected': count, 'executed': count, 'skipped': 0, 'failures': 0, 'errors': 0}:
        raise ValueError('Original native selected count is incomplete')
    counts = native.get('test_counts', {})
    if (counts.get('selected') != count or counts.get('executed') != count
        or counts.get('skipped') != 0 or len(counts.get('cases', [])) != count
        or any(case.get('skipped') is not False for case in counts['cases'])):
        raise ValueError('Original native cases are incomplete')
    junit = native.get('junit', {})
    path = plain(campaign / 'evidence/nextest-store/ci/junit.xml', False)
    if str(path) != junit.get('path') or path.stat().st_size != junit.get('bytes') or sha(path) != junit.get('sha256'):
        raise ValueError('Original native JUnit proof changed')
    cases = list(ET.parse(path).iter('testcase'))
    if len(cases) != count or any(any(case.find(tag) is not None for tag in ('failure', 'error', 'skipped')) for case in cases):
        raise ValueError('Original native JUnit has zero, failed or skipped cases')
    for name, digest in approved['logs'].items():
        if sha(plain(campaign / 'evidence' / name, False)) != digest:
            raise ValueError('Original stage log changed: ' + name)
    for record in receipt.get('retained_cargo_timings', []):
        path = plain(record['path'], False)
        if not path.is_relative_to(campaign / 'evidence/cargo-timings') or sha(path) != record['sha256'] or path.stat().st_size != record['bytes']:
            raise ValueError('Original retained native timing proof changed')
    if not receipt.get('retained_cargo_timings'):
        raise ValueError('Original native timing proof missing')
    target = campaign / 'cargo-target'
    if target.is_symlink() or target.exists() and (plain(target, True) != target or any(target.iterdir())):
        raise ValueError('Original compiler target is not cleaned')
    return receipt


def validate_frozen(campaign, prior, native_observer, approved=APPROVED):
    source = plain(campaign / 'source', True)
    manifest_path = campaign / 'source-manifest.json'
    manifest = read_json(manifest_path)
    models = read_json(campaign / 'public-model-transfer-manifest.json')
    if (sha(manifest_path) != approved['source_manifest_sha256']
        or manifest.get('status') != 'FROZEN_PUBLIC_SOURCE' or manifest.get('final_freeze_required') is not False
        or manifest.get('native_source') != approved['native']
        or manifest.get('source_roster_sha256') != approved['source_roster_sha256']
        or manifest.get('file_count') != approved['source_file_count']
        or manifest.get('file_count') != len(manifest.get('files', []))
        or manifest.get('public_models_roster_sha256') != approved['models_roster_sha256']
        or models.get('model_roster_sha256') != approved['models_roster_sha256']):
        raise ValueError('Frozen source/model manifest changed')
    verify_rows(source, manifest['files'], approved['source_roster_sha256'])
    verify_rows(plain(campaign / 'offline-model', True), models['files'], approved['models_roster_sha256'])
    if native_observer() != approved['native'] or prior['source_identity'] != approved['native']:
        raise ValueError('Frozen native inputs changed')
    if sha(plain(campaign / 'neo-scoped-owner.py', False)) != approved['original_runner_sha256']:
        raise ValueError('Original runner changed')
    return {'native': approved['native'], 'source_roster_sha256': approved['source_roster_sha256'],
            'public_models_roster_sha256': approved['models_roster_sha256']}


def create_namespace(campaign, name):
    if not re.fullmatch(r'components-followup-r36-[0-9]+', name):
        raise ValueError('Invalid fresh follow-up namespace')
    campaign = plain(campaign, True)
    if hasattr(os, 'getuid') and campaign.stat().st_uid != os.getuid():
        raise ValueError('Campaign has a foreign owner')
    namespace = campaign / name
    namespace.mkdir(mode=0o700, exist_ok=False)
    if plain(namespace, True).parent != campaign:
        raise ValueError('Crossed follow-up namespace')
    (namespace / 'fixtures').mkdir(mode=0o700)
    return namespace


def prepare_artifacts(source):
    source = plain(source, True)
    artifacts = source / 'artifacts'
    created = not artifacts.exists()
    if artifacts.is_symlink():
        raise ValueError('Source artifacts redirects outside the frozen checkout')
    if created:
        artifacts.mkdir(mode=0o700)
    plain(artifacts, True)
    if hasattr(os, 'getuid') and artifacts.stat().st_uid != os.getuid():
        raise ValueError('Source artifacts has a foreign owner')
    return {'path': str(artifacts), 'created_by_followup': created,
            'removed': False, 'scope': 'ignored test parent only; existing contents retained'}


def python_counts(text, expected=919):
    rows = re.findall(r'^Ran (\d+) tests? in ', text, re.MULTILINE)
    if rows != [str(expected)] or not re.search(r'^OK(?: \(skipped=\d+\))?\s*$', text, re.MULTILINE) or re.search(r'^FAILED', text, re.MULTILINE):
        raise ValueError('Python full suite omitted its exact positive passing count')
    skipped = re.findall(r'^OK \(skipped=(\d+)\)', text, re.MULTILINE)
    count = int(skipped[0]) if skipped else 0
    if expected <= 0 or count >= expected:
        raise ValueError('Python executed zero cases')
    return {'selected': expected, 'skipped': count, 'executed': expected - count}


def node_counts(text):
    result = {}
    for name in ('tests', 'pass', 'fail', 'cancelled', 'skipped', 'todo'):
        rows = re.findall(r'^(?:[\u2139#]\s+)?' + name + r'\s+(\d+)\s*$', text, re.MULTILINE)
        if len(rows) != 1:
            raise ValueError('Node omitted its complete count: ' + name)
        result[name] = int(rows[0])
    if (result['tests'] <= 0 or result['pass'] <= 0 or result['fail'] or result['cancelled'] or result['todo']
        or result['pass'] + result['skipped'] != result['tests']):
        raise ValueError('Node selected zero or has failed/uncertain cases')
    return result


def compiled_inputs(source):
    out = plain(source / 'packages/vscode/out', True)
    model = plain(out / 'model.js', False)
    if not model.stat().st_size:
        raise ValueError('Python real consumer needs compiled model.js')
    rows = []
    for path in sorted(out.rglob('*')):
        plain(path)
        if path.is_file():
            if len(rows) >= 10000:
                raise ValueError('Compiled extension roster exceeds bound')
            rows.append({'path': path.relative_to(out).as_posix(), 'bytes': path.stat().st_size, 'sha256': sha(path)})
    tests = sorted(out.glob('test/*.test.js'))
    if not tests:
        raise ValueError('Compiled extension selected zero test files')
    return {'files': rows, 'sha256': hashlib.sha256(canonical(rows)).hexdigest(), 'test_files': [str(path) for path in tests]}


def run_components(source, namespace, owner, environment, verify, state, save, clock=time.monotonic):
    """Inject only command ownership/clock for cheap cold-checkout regressions."""
    verify()
    setup = {'stage': REQUIRED_GATES[0], 'owned_command_started': False, 'result': 'running'}
    state['commands'].append(setup)
    save()
    try:
        setup.update(prepare_artifacts(source))
        verify()
        setup['result'] = 'passed'
        save()
    except BaseException as error:
        setup.update(result='failed', error=str(error))
        save()
        raise
    plan = [
        (REQUIRED_GATES[1], ['npm', 'ci', '--prefix', 'packages/vscode', '--prefer-offline', '--ignore-scripts', '--no-audit', '--no-fund'], 600),
        (REQUIRED_GATES[2], ['npm', 'run', 'compile', '--prefix', 'packages/vscode'], 600),
        (REQUIRED_GATES[3], [sys.executable, '-m', 'unittest', 'discover', '-s', 'scripts/tests', '-v'], 1800),
        (REQUIRED_GATES[4], None, 1800),
    ]
    compiled = None
    for stage, argv, timeout in plan:
        row = {'stage': stage, 'result': 'running', 'owned_command_started': False,
               'stop_condition': 'first failure, input change, timeout, uncertain descendants or incomplete counts'}
        state['commands'].append(row)
        save()
        started = clock()
        try:
            row['frozen_inputs_before'] = verify()
            if compiled is not None and compiled_inputs(source) != compiled:
                raise ValueError('Compiled extension inputs changed')
            if argv is None:
                argv = ['node', '--test', '--test-concurrency=2', *compiled['test_files']]
            row.update(argv=argv, cwd=str(source), owned_command_started=True)
            result = owner.run_owned(argv, source, namespace / (stage + '.log'), timeout, environment)
            row.update(result)
            if not owner.successful(result):
                raise ValueError(stage + ' command failed or descendants unsettled')
            row['frozen_inputs_after'] = verify()
            if stage == REQUIRED_GATES[2]:
                compiled = compiled_inputs(source)
                state['compiled_extension_inputs'] = compiled
            elif compiled is not None and compiled_inputs(source) != compiled:
                raise ValueError('Compiled extension inputs changed')
            if stage == REQUIRED_GATES[3]:
                row['test_counts'] = python_counts((namespace / (stage + '.log')).read_text(), APPROVED['python_selected'])
            elif stage == REQUIRED_GATES[4]:
                row['test_counts'] = node_counts((namespace / (stage + '.log')).read_text())
            row['result'] = 'passed'
        except BaseException as error:
            row.update(result='failed', error=str(error))
            raise
        finally:
            row['elapsed_seconds'] = round(clock() - started, 3)
            save()
    return compiled


def load(source, name, file):
    spec = importlib.util.spec_from_file_location(name, source / 'scripts' / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--campaign', type=Path, required=True)
    parser.add_argument('--namespace', required=True)
    args = parser.parse_args(argv)
    if (sys.flags.optimize or platform.system() != 'Darwin' or platform.machine() != 'arm64'
        or os.getuid() != 501 or socket.gethostname() != 'Tylers-MacBook-Neo.local'):
        raise ValueError('Only the established nonoptimized UID501 Neo ARM host is allowed')
    campaign = plain(args.campaign, True)
    if campaign.parent != ROOT:
        raise ValueError('Original campaign escaped the approved Neo root')
    prior = validate_prior(campaign)
    source = plain(campaign / 'source', True)
    # Authenticate every importable frozen script before executing an import.
    validate_frozen(campaign, prior, lambda: APPROVED['native'])
    builder = load(source, 'r36_component_builder', 'build-service-runtime.py')
    owner = load(source, 'r36_component_owned_commands', 'provision-static-onnx-runtime.py')
    runner = plain(Path(__file__), False)
    runner_digest = sha(runner)

    def verify():
        validate_prior(campaign)
        result = validate_frozen(campaign, prior, builder.source_identity)
        if sha(runner) != runner_digest:
            raise ValueError('Follow-up runner changed')
        return result

    verify()
    namespace = create_namespace(campaign, args.namespace)
    fixtures = namespace / 'fixtures'
    state = {'schema': 1, 'purpose': 'remaining Python and mocked extension components on original frozen Neo source',
             'result': 'running', 'qualification_complete': False, 'namespace': str(namespace),
             'original_batch_receipt': str(campaign / 'evidence/batch-receipt.json'),
             'original_batch_sha256': APPROVED['prior_sha256'], 'original_batch_result': 'failed',
             'previous_native_qualification': {'native_source': APPROVED['native'], 'native_tests': 393,
                 'scope': 'original scoped Clippy and 393 native tests retained; not rerun or relabeled'},
             'runner_sha256': runner_digest, 'source': str(source), 'commands': [],
             'limits': ['No Cargo/build/clean repeated', 'No native installed/editor/signing/release qualification from this follow-up']}
    started = time.monotonic()

    def save():
        state['elapsed_seconds'] = round(time.monotonic() - started, 3)
        (namespace / 'component-receipt.json').write_text(json.dumps(state, indent=2) + '\n')

    try:
        excluded = {'PYTHONOPTIMIZE', 'PYTHONPATH', 'PYTHONHOME', 'PYTHONSTARTUP', 'PYTHONINSPECT',
                    'GIT_DIR', 'GIT_WORK_TREE', 'GIT_INDEX_FILE', 'GIT_COMMON_DIR', 'GIT_OBJECT_DIRECTORY',
                    'GIT_ALTERNATE_OBJECT_DIRECTORIES', 'GIT_REPLACE_REF_BASE', 'GIT_NAMESPACE', 'GIT_CONFIG',
                    'GIT_CONFIG_COUNT', 'GIT_CONFIG_PARAMETERS', 'CARGO_TARGET_DIR'}
        environment = {key: value for key, value in os.environ.items()
                       if key not in excluded and not key.startswith(('AIRK_', 'GIT_CONFIG_KEY_', 'GIT_CONFIG_VALUE_'))}
        environment['PATH'] = ':'.join([str(TOOLS / 'node-v24.15.0-darwin-arm64/bin'), str(TOOLS),
                                        str(Path(sys.executable).parent), environment.get('PATH', '')])
        environment.update(AIRK_BUNDLED_MODEL_DIR=str(campaign / 'offline-model'),
            AIRK_TEST_MODEL_DIR=str(campaign / 'offline-model/minilm-l6-v2-s8-v1'),
            AIRK_PRIVATE_CENSUS_BASE=str(fixtures), TMPDIR=str(fixtures), TEMP=str(fixtures), TMP=str(fixtures),
            PYTHONNOUSERSITE='1', PYTHONDONTWRITEBYTECODE='1')
        for tool in ('node', 'npm'):
            selected = shutil.which(tool, path=environment['PATH'])
            selected = Path(selected).resolve(strict=True) if selected else None
            if selected is None or not plain(selected, False).is_relative_to(TOOLS / 'node-v24.15.0-darwin-arm64'):
                raise ValueError('Pinned Neo Node/npm prerequisite unavailable: ' + tool)
        run_components(source, namespace, owner, environment, verify, state, save)
        state['result'] = 'passed'
    except BaseException as error:
        state.update(result='failed', error={'type': type(error).__name__, 'message': str(error)[:4096]})
    finally:
        rows = state['commands']
        state['all_owned_command_descendants_settled'] = all(
            row.get('owned_command_started') is False or row.get('owned_descendants_settled') is True for row in rows)
        state['qualification_complete'] = bool(state['result'] == 'passed'
            and state['all_owned_command_descendants_settled']
            and [row.get('stage') for row in rows] == REQUIRED_GATES
            and all(row.get('result') == 'passed' for row in rows))
        if not state['qualification_complete']:
            state['result'] = 'failed'
        save()
        (namespace / 'phase.txt').write_text('complete-' + state['result'])
        print(json.dumps({'result': state['result'], 'receipt': str(namespace / 'component-receipt.json')}))
    return 0 if state['qualification_complete'] else 1


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except Exception as error:
        print(type(error).__name__ + ': ' + str(error)[:4096], file=sys.stderr)
        raise SystemExit(1)
