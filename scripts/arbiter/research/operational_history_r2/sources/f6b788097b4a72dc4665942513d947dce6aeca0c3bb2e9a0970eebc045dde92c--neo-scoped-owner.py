# SPDX-License-Identifier: Apache-2.0
"""One fresh frozen-source Neo ARM R38 component batch; no installed-release claim."""
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
import stat
import sys
import tempfile
import time

ROOT = Path('/Users/tylerjensen/airk-builds')
TOOLS = ROOT / 'timing-1.6.4.PutU1P'
PRIOR_NATIVE_RECEIPT = ROOT / 'airk-clean-break-r35-final-1794224190256027000/evidence/batch-receipt.json'
PRIOR_NATIVE_SHA256 = 'deee97c43cc38975a71cf1ce64755b9682d314771d5851ec99990a1a4e7af54a'
REQUIRED_GATES = ['extension-locked-dependencies', 'extension-full-compile',
                  'full-python-contracts', 'full-mocked-extension-tests']

def socket_budget(base):
    from pathlib import PurePosixPath
    endpoint = PurePosixPath(str(base)) / 'airk-explorer-XXXXXX' / 'airk-explorer.sock'
    size = len(str(endpoint).encode('utf-8')) + 1
    if not endpoint.is_absolute() or size > 104:
        raise ValueError('Darwin Assistance socket exceeds its 104-byte sun_path budget including NUL')
    return size


def validate_fixture_root(path, canonical, uid, mode, linked):
    from pathlib import PurePosixPath
    path, canonical = PurePosixPath(str(path)), PurePosixPath(str(canonical))
    if (path != canonical or path.parent != PurePosixPath('/private/tmp') or linked
        or uid != 501 or mode != 0o700 or not re.fullmatch('airk-r38-[A-Za-z0-9_-]{6,32}', path.name)):
        raise ValueError('Fixture root is foreign, public, redirected or outside the exact R38 short namespace')
    return {'path': str(path), 'owner_uid': uid, 'mode': '0700',
            'socket_bytes_including_nul': socket_budget(path), 'socket_limit': 104}


def inspect_fixture_root(path):
    metadata = path.lstat()
    if not stat.S_ISDIR(metadata.st_mode):
        raise ValueError('Owned fixture root is not a directory')
    return validate_fixture_root(path, path.resolve(strict=True), metadata.st_uid,
                                 stat.S_IMODE(metadata.st_mode), path.is_symlink())


def create_fixture_root():
    parent = Path('/private/tmp')
    if parent.is_symlink() or parent.resolve(strict=True) != parent:
        raise ValueError('Private temporary parent is redirected')
    path = Path(tempfile.mkdtemp(prefix='airk-r38-', dir=parent))
    inspect_fixture_root(path)
    return path


def prepare_artifacts(source):
    path = source / 'artifacts'
    created = not path.exists()
    if path.is_symlink():
        raise ValueError('Ignored artifacts parent is redirected')
    if created:
        path.mkdir(mode=0o700)
    metadata = path.lstat()
    if not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid != 501 or path.resolve(strict=True) != path:
        raise ValueError('Ignored artifacts parent has a foreign owner or path')
    return {'path': str(path), 'created': created, 'existing_contents_retained': True}


def validate_assistance_factory(source):
    host = (source / 'packages/vscode/src/explorerAssistanceHost.ts').read_text(encoding='utf-8')
    if ("fs.mkdtempSync(path.join(os.tmpdir(), 'airk-explorer-'))" not in host
        or "path.join(socketDirectory, 'airk-explorer.sock')" not in host):
        raise ValueError('Frozen Assistance socket factory needs a new path-budget review')


def compiled_inputs(source, expected_tests):
    test_source = source / 'packages/vscode/src/test'
    out = source / 'packages/vscode/out'
    for root in (test_source, out):
        if root.is_symlink() or not root.is_dir() or root.resolve(strict=True) != root:
            raise ValueError('Extension input root is missing or redirected')
    sources = {}
    expected = []
    for path in sorted(test_source.rglob('*')):
        if path.is_symlink() or path.resolve(strict=True) != path:
            raise ValueError('Redirected test source entry')
        if not path.is_file():
            if not path.is_dir():
                raise ValueError('Special test source entry')
            continue
        if not path.name.endswith('.test.ts'):
            continue
        if path.parent != test_source:
            raise ValueError('Unexpected test source location')
        sources[path.relative_to(source).as_posix()] = sha(path)
        expected.append(str(out / 'test' / (path.stem + '.js')))
    if expected_tests <= 0 or len(sources) != expected_tests:
        raise ValueError('Live source test catalog differs from frozen explicit count')
    rows = []
    for path in sorted(out.rglob('*')):
        if path.is_symlink() or path.resolve(strict=True) != path:
            raise ValueError('Compiled extension input is redirected')
        if path.is_file():
            if path.name.endswith('.test.js') and (out / 'test') in path.parents and path.parent != out / 'test':
                raise ValueError('Nested compiled test would be omitted from selection')
            if len(rows) >= 10000:
                raise ValueError('Compiled extension input roster exceeds its bound')
            rows.append({'path': path.relative_to(out).as_posix(),
                         'bytes': path.stat().st_size, 'sha256': sha(path)})
        elif not path.is_dir():
            raise ValueError('Compiled extension contains a special input')
    tests = sorted(out.glob('test/*.test.js'))
    if (not (out / 'model.js').is_file() or not (out / 'model.js').stat().st_size
        or [str(path) for path in tests] != sorted(expected)):
        raise ValueError('Compiled Python consumer or nonzero full mocked test selection is missing')
    return {'files': rows, 'sha256': hashlib.sha256(json.dumps(rows, sort_keys=True,
             separators=(',', ':')).encode()).hexdigest(), 'test_files': [str(path) for path in tests],
             'test_sources': sources, 'selected_test_files': len(tests)}


def require_compiled_inputs(source, expected, expected_tests):
    if compiled_inputs(source, expected_tests) != expected:
        raise ValueError('Compiled extension inputs changed after the frozen compile')


def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def load(source, name, filename):
    specification = importlib.util.spec_from_file_location(name, source / 'scripts' / filename)
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def require_python_count(log):
    import re
    rows = re.findall(r'^Ran (\d+) tests? in ', log.read_text(encoding='utf-8', errors='replace'), re.MULTILINE)
    if len(rows) != 1 or int(rows[0]) <= 0:
        raise ValueError('Python must report one nonzero selected-test count')
    return {'selected': int(rows[0]), 'summary': [line for line in log.read_text(encoding='utf-8', errors='replace').splitlines()
                                               if line.startswith(('Ran ', 'OK', 'FAILED'))]}


def require_node_count(log, require_pass=True):
    import re
    text = log.read_text(encoding='utf-8', errors='replace')
    values = {}
    for name in ('tests', 'pass', 'fail', 'skipped', 'cancelled'):
        matches = re.findall(r'^(?:[\u2139#]\s+)?' + name + r'\s+(\d+)\s*$', text, re.MULTILINE)
        if len(matches) != 1:
            raise ValueError('Node omitted its complete test summary: ' + name)
        values[name] = int(matches[0])
    if require_pass and (values['tests'] <= 0 or values['pass'] <= 0 or values['fail'] or values['cancelled']):
        raise ValueError('Mocked extension suite must execute nonzero tests without failure/cancellation')
    return values


def prior_native_qualification(expected):
    path = PRIOR_NATIVE_RECEIPT
    if path.is_symlink() or path.resolve(strict=True) != path or sha(path) != PRIOR_NATIVE_SHA256:
        raise ValueError('Prior R37 native qualification receipt changed or redirected')
    value = json.loads(path.read_text())
    if (value.get('qualification_revision') != 'R37' or value.get('result') != 'passed'
        or value.get('qualification_complete') is not True or value.get('source_identity') != expected
        or value.get('cargo_cleanup') != 'completed' or value.get('compiler_tree_absent_or_empty') is not True
        or value.get('all_owned_command_descendants_settled') is not True):
        raise ValueError('Prior R37 native qualification or cleanup is incomplete or crossed')
    commands = value.get('commands', [])
    for stage in ('scoped-four-crate-clippy', 'focused-clean-break-native-tests', 'required-neo-cargo-clean'):
        rows = [row for row in commands if row.get('stage') == stage]
        if len(rows) != 1 or rows[0].get('result') != 'passed' or rows[0].get('owned_descendants_settled') is not True:
            raise ValueError('Prior native gate or settlement proof missing: ' + stage)
        if stage == 'focused-clean-break-native-tests':
            counts = rows[0].get('test_counts', {})
            if counts.get('selected') != 398 or counts.get('executed') != 398 or counts.get('skipped') != 0:
                raise ValueError('Prior R37 native selected-test proof differs from the actual 398-test run')
    return {'receipt': str(path), 'sha256': PRIOR_NATIVE_SHA256, 'native_source': expected,
            'revision': 'R37', 'selected_native_tests': 398, 'reused_prior_qualification': True,
            'cargo_cleanup': 'completed', 'new_native_commands': False}


def finalize_qualification(state):
    rows = state.get('commands', [])
    settled = all(row.get('owned_command_started') is False or row.get('owned_descendants_settled') is True for row in rows)
    gates = [row.get('stage') for row in rows] == REQUIRED_GATES and all(row.get('result') == 'passed' for row in rows)
    state['all_owned_command_descendants_settled'] = settled
    state['qualification_complete'] = bool(state.get('result') == 'passed' and gates and settled
        and state.get('prior_native_qualification') and state.get('frozen_inputs_final')
        and state.get('compiler_tree_absent_or_empty') is True
        and state.get('cargo_cleanup') == 'no-owned-compiler-target-created')
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
    if campaign.parent != ROOT.resolve(strict=True) or not re.fullmatch(r'airk-clean-break-r35-final-[0-9]+', campaign.name):
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
               'qualification_scope': 'R38 Python and mocked extension components only', 'campaign': str(campaign),
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
        raise ValueError('Optimized Python discards ownership assertions')
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--campaign', type=Path, required=True)
    parser.add_argument('--native-sha256', required=True)
    parser.add_argument('--expected-test-files', type=int, required=True)
    args = parser.parse_args(argv)
    if platform.system() != 'Darwin' or platform.machine() != 'arm64' or os.getuid() != 501 or socket.gethostname() != 'Tylers-MacBook-Neo.local':
        raise ValueError('This owner is only for the established UID501 Neo ARM host')
    import resource
    supplied = args.campaign.absolute()
    campaign = supplied.resolve(strict=True)
    if supplied != campaign or campaign.parent != ROOT or not re.fullmatch(r'airk-clean-break-r35-final-[0-9]+', campaign.name):
        raise ValueError('Choose only a fresh reviewed isolated campaign')
    source = campaign / 'source'
    if source.resolve(strict=True) != source or source.is_symlink():
        raise ValueError('Frozen source checkout is redirected')
    evidence, target = campaign / 'evidence', campaign / 'cargo-target'
    if any(path.exists() or path.is_symlink() for path in (evidence, target, source / 'target')):
        raise ValueError('Do not borrow another evidence/compiler target owner')
    evidence.mkdir(mode=0o700)
    fixtures = create_fixture_root()
    started = time.monotonic()
    state = {'schema': 1, 'qualification_revision': 'R38', 'result': 'running', 'qualification_complete': False,
             'purpose': 'component-only frozen R38 extension/Python/mockNode delta on Neo ARM',
             'qualification_scope': 'R38 Python and mocked extension components; R37 native proof retained separately',
             'host': socket.gethostname(), 'uid': os.getuid(), 'architecture': platform.machine(),
             'source': str(source), 'fixtures': str(fixtures), 'fixture_root_retained': True,
             'fixture_root_identity': inspect_fixture_root(fixtures), 'commands': [],
             'native_commands_invoked': False, 'compiler_target_created': False,
             'limits': ['No new native, installed Mac editor/runtime, GUI, Intel, signing or public release qualification']}
    compiled = None
    frozen_inputs = None

    def save():
        state['elapsed_seconds'] = round(time.monotonic() - started, 3)
        (evidence / 'batch-receipt.json').write_text(json.dumps(state, indent=2) + '\n')

    try:
        manifest_path = campaign / 'source-manifest.json'
        manifest = json.loads(manifest_path.read_text())
        models_path = campaign / 'public-model-transfer-manifest.json'
        models = json.loads(models_path.read_text())
        if manifest.get('status') != 'FROZEN_PUBLIC_SOURCE' or manifest.get('final_freeze_required') is not False:
            raise ValueError('Only finalized reviewed public inputs may run')
        if manifest['native_source']['sha256'] != args.native_sha256:
            raise ValueError('Restored native source differs from the explicit pin')
        state.update(source_identity=manifest['native_source'], source_roster_sha256=manifest['source_roster_sha256'],
            source_manifest_sha256=sha(manifest_path), public_models_roster_sha256=models['model_roster_sha256'],
            public_models_manifest_sha256=sha(models_path), runner_sha256=sha(Path(__file__).resolve(strict=True)),
            git_producer=manifest['head'])
        if manifest['public_models_roster_sha256'] != models['model_roster_sha256']:
            raise ValueError('Frozen source and public model manifests disagree')
        state['prior_native_qualification'] = prior_native_qualification(state['source_identity'])
        builder = load(source, 'r38_builder_identity_only', 'build-service-runtime.py')
        owner = load(source, 'r38_owned_commands', 'provision-static-onnx-runtime.py')

        def frozen_inputs():
            inspect_fixture_root(fixtures)
            if target.exists() or target.is_symlink() or (source / 'target').exists() or (source / 'target').is_symlink():
                raise ValueError('Component-only batch unexpectedly created a Cargo target; preserve it without cleanup')
            if compiled is not None:
                require_compiled_inputs(source, compiled, args.expected_test_files)
            if (sha(manifest_path) != state['source_manifest_sha256'] or sha(models_path) != state['public_models_manifest_sha256']
                or sha(Path(__file__).resolve(strict=True)) != state['runner_sha256']):
                raise ValueError('Frozen manifest or runner changed')
            if builder.source_identity() != state['source_identity']:
                raise ValueError('Native inputs changed; prior qualification cannot be reused')
            prior_native_qualification(state['source_identity'])
            for base, rows in ((source, manifest['files']), (campaign / 'offline-model', models['files'])):
                for row in rows:
                    relative = Path(row['path'])
                    if relative.is_absolute() or '..' in relative.parts:
                        raise ValueError('Frozen input path escaped its public root')
                    path = base / relative
                    if (path.is_symlink() or path.resolve(strict=True) != path or not path.is_file()
                        or path.stat().st_size != row['bytes'] or sha(path) != row['sha256']):
                        raise ValueError('Frozen public input changed: ' + row['path'])
            return {'native': state['source_identity'], 'source_roster_sha256': state['source_roster_sha256'],
                    'public_models_roster_sha256': state['public_models_roster_sha256'], 'runner_sha256': state['runner_sha256']}

        frozen_inputs()
        validate_assistance_factory(source)
        forbidden = {'PYTHONOPTIMIZE','PYTHONPATH','PYTHONHOME','PYTHONSTARTUP','PYTHONINSPECT',
            'GIT_DIR','GIT_WORK_TREE','GIT_INDEX_FILE','GIT_COMMON_DIR','GIT_OBJECT_DIRECTORY',
            'GIT_ALTERNATE_OBJECT_DIRECTORIES','GIT_REPLACE_REF_BASE','GIT_NAMESPACE','GIT_CONFIG','GIT_CONFIG_COUNT','GIT_CONFIG_PARAMETERS',
            'CARGO_TARGET_DIR','CARGO_BUILD_TARGET_DIR'}
        environment = {key:value for key,value in os.environ.items()
            if key not in forbidden and not key.startswith(('AIRK_','GIT_CONFIG_KEY_','GIT_CONFIG_VALUE_'))}
        environment['PATH'] = ':'.join([str(TOOLS / 'tools/node-v24.15.0-darwin-arm64/bin'),
            str(TOOLS / 'tools'), str(Path(sys.executable).parent), environment.get('PATH','')])
        environment.update(AIRK_BUNDLED_MODEL_DIR=str(campaign / 'offline-model'),
            AIRK_TEST_MODEL_DIR=str(campaign / 'offline-model/minilm-l6-v2-s8-v1'),
            AIRK_PRIVATE_CENSUS_BASE=str(fixtures), TMPDIR=str(fixtures), TEMP=str(fixtures), TMP=str(fixtures),
            PYTHONNOUSERSITE='1', PYTHONDONTWRITEBYTECODE='1')
        missing = [name for name in ('node','npm') if not shutil.which(name, path=environment['PATH'])]
        if missing:
            raise ValueError('Required component tools unavailable: ' + ','.join(missing))
        soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
        if hard < 4096:
            raise ValueError('Owned shell cannot supply required descriptor limit')
        resource.setrlimit(resource.RLIMIT_NOFILE, (4096, hard))
        state['nofile'] = {'before':[soft,hard], 'after':list(resource.getrlimit(resource.RLIMIT_NOFILE))}

        def run(name, command, timeout, count=None):
            (campaign / 'phase.txt').write_text(name)
            row = {'stage':name, 'argv':[str(value) for value in command], 'cwd':str(source),
                   'started_unix':time.time(), 'result':'running', 'owned_command_started':False,
                   'expected_result':'exit zero, complete counts and unchanged frozen inputs',
                   'stop_condition':'first failure, uncertain settlement or input drift'}
            state['commands'].append(row)
            save()
            before = time.monotonic()
            try:
                row['frozen_inputs_before'] = frozen_inputs()
                row['owned_command_started'] = True
                row.update(owner.run_owned(row['argv'], source, evidence / (name + '.log'), timeout, environment))
                row['elapsed_seconds'] = round(time.monotonic() - before, 3)
                row['result'] = 'passed' if owner.successful(row) else 'failed'
                row['frozen_inputs_after'] = frozen_inputs()
                if count:
                    try:
                        row['observed_test_counts'] = (require_python_count(evidence / (name + '.log')) if count == 'python'
                            else require_node_count(evidence / (name + '.log'), require_pass=False))
                    except ValueError as error:
                        row['count_evidence_error'] = str(error)
                if row['result'] == 'passed' and count == 'python':
                    row['test_counts'] = require_python_count(evidence / (name + '.log'))
                elif row['result'] == 'passed' and count == 'node':
                    row['test_counts'] = require_node_count(evidence / (name + '.log'))
                save()
                if row['result'] != 'passed':
                    raise ValueError(name + ' failed; preserve the complete batch before any correction')
            except BaseException as error:
                row.update(result='failed', error=str(error), elapsed_seconds=round(time.monotonic() - before, 3))
                save()
                raise

        run('extension-locked-dependencies', ['npm','ci','--prefix','packages/vscode','--prefer-offline','--ignore-scripts','--no-audit','--no-fund'], 600)
        run('extension-full-compile', ['npm','run','compile','--prefix','packages/vscode'], 600)
        compiled = compiled_inputs(source, args.expected_test_files)
        state['compiled_extension_inputs'] = compiled
        state['artifacts_parent'] = prepare_artifacts(source)
        save()
        run('full-python-contracts', [sys.executable,'-m','unittest','discover','-s','scripts/tests','-v'], 1800, 'python')
        run('full-mocked-extension-tests', ['node','--test','--test-concurrency=2',*compiled['test_files']], 1800, 'node')
        state['result'] = 'passed'
    except BaseException as error:
        state.update(result='failed', error=str(error))
    finally:
        try:
            state['frozen_inputs_final'] = frozen_inputs() if frozen_inputs is not None else None
        except BaseException as error:
            state.update(result='failed', final_input_error=str(error))
        absent = not any(path.exists() or path.is_symlink() for path in (target, source / 'target'))
        state['compiler_tree_absent_or_empty'] = absent
        state['cargo_cleanup'] = 'no-owned-compiler-target-created' if absent else 'unexpected-target-preserved-no-cleanup-authority'
        state['cleanup_note'] = 'No Cargo invocation or owned target was created; retained R37 target-clean receipt is separate prior evidence.'
        state['source_and_fixtures_retained'] = source.is_dir() and fixtures.is_dir()
        finalize_qualification(state)
        save()
        (campaign / 'phase.txt').write_text('complete-' + state['result'])
        print(json.dumps({'result':state['result'],'receipt':str(evidence / 'batch-receipt.json'),
                          'cargo_cleanup':state['cargo_cleanup']}), flush=True)
    return 0 if state['result'] == 'passed' else 1

if __name__ == '__main__':
    raise SystemExit(entrypoint())
