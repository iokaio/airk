# SPDX-License-Identifier: Apache-2.0
"""Run only the remaining frozen Neo mocked Node suite with short private Unix socket fixtures."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import socket
import stat
import sys
import tempfile
import time

ROOT = Path('/Users/tylerjensen/airk-builds')
SHORT_PARENT = Path('/private/tmp')
CAMPAIGN = 'airk-clean-break-r35-final-1794218081236320400'
COMPONENT = 'components-followup-r36-1794219736000000000'
COMPONENT_HELPER = 'components-control-r36-1794219736000000000/neo-components-followup-r36.py'
EXPECTED = {
    'component': COMPONENT,
    'receipt_sha256': '86b4433e0cb053f37f27b320adbe92a5967b1aa4dd1dcd524fcdba1cc4f79a39',
    'runner_sha256': '8046d49a2e2b4d5e64704abc006217f2f0f48cf83fdc83eff588c6e47ffe4dd7',
    'compiled_sha256': 'bff2be1fdc783d3051d44def813ab66756999acca1dbabd538d1b34ca332cd06',
    'compiled_files': 209, 'compiled_test_files': 104,
    'logs': {
        'extension-locked-dependencies.log': 'e10e0d47ceaf77341348e87ccc3b63684a3026e88fc5631a8978e6044c2fd08f',
        'extension-full-compile.log': '53c9ee254250ad207c2aa830b98bd0bb45c364b4ee13282f37ea19bde7b2715a',
        'full-python-contracts.log': 'd276fb2907af959f13f4bcf38f803704e825ddb60faaa81f62c098a2069e30ce',
        'full-mocked-extension-tests.log': '75b7d3d85c4f896569e5af8537e816855d61e7a54c177c3fd64f4d955045828f',
    },
}


def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def component_helper_path(campaign):
    # Exact argv[2] in the retained component launch.json; never synthesize a
    # campaign-root copy or search another control namespace for matching bytes.
    return campaign / COMPONENT_HELPER


def load_verified(path, digest, name):
    path = Path(path)
    if (any(item.is_symlink() for item in (path, *path.parents))
        or not path.is_file() or sha(path) != digest):
        raise ValueError('Reviewed component helper identity changed')
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def socket_budget(base):
    """All four failing suites call the same AssistanceIpcHost socket factory."""
    base = PurePosixPath(str(base))
    endpoint = base / 'airk-explorer-XXXXXX' / 'airk-explorer.sock'
    size = len(str(endpoint).encode('utf-8')) + 1
    if not base.is_absolute() or size > 104:
        raise ValueError(f'Darwin Assistance socket exceeds sun_path budget: {size} bytes including NUL')
    return {'endpoint_template': str(endpoint), 'bytes_including_nul': size, 'limit': 104,
            'shared_factory_consumers': ['explorerAssistanceHost', 'explorerAssistanceTools', 'serviceOnboarding', 'trust']}


def validate_short_root(path, canonical_path, owner_uid, expected_uid, mode, linked, parent='/private/tmp'):
    """Pure cross-platform ownership/path/budget decision; no chmod or fallback."""
    path, canonical_path, parent = map(lambda value: PurePosixPath(str(value)), (path, canonical_path, parent))
    if (path != canonical_path or path.parent != parent or linked or owner_uid != expected_uid
        or mode != 0o700
        or not re.fullmatch(r'airk-n36-[A-Za-z0-9_-]{6,32}', path.name)):
        raise ValueError('Short fixture root is redirected, public, foreign or outside its exact namespace')
    return socket_budget(path)


def observe_short_root(path):
    path = Path(path)
    metadata = path.lstat()
    if not stat.S_ISDIR(metadata.st_mode):
        raise ValueError('Short fixture root is not a directory')
    return validate_short_root(path, path.resolve(strict=True), metadata.st_uid, os.getuid(),
                               stat.S_IMODE(metadata.st_mode), path.is_symlink())


def create_short_root():
    if SHORT_PARENT.resolve(strict=True) != SHORT_PARENT or SHORT_PARENT.is_symlink():
        raise ValueError('Private temporary parent is redirected')
    path = Path(tempfile.mkdtemp(prefix='airk-n36-', dir=SHORT_PARENT))
    # mkdtemp creates 0700. Refuse a changed owner/mode; never repair an old root.
    observe_short_root(path)
    return path


def node_summary(text):
    result = {}
    for name in ('tests', 'pass', 'fail', 'cancelled', 'skipped', 'todo'):
        values = re.findall(r'^(?:[\u2139#]\s+)?' + name + r'\s+(\d+)\s*$', text, re.MULTILINE)
        if len(values) != 1:
            raise ValueError('Missing complete Node count: ' + name)
        result[name] = int(values[0])
    if result['tests'] <= 0 or sum(result[key] for key in ('pass', 'fail', 'cancelled', 'skipped', 'todo')) != result['tests']:
        raise ValueError('Zero or inconsistent full Node selection')
    return result


def require_node_pass(text):
    counts = node_summary(text)
    if counts != {'tests': 1170, 'pass': 1137, 'fail': 0, 'cancelled': 0, 'skipped': 33, 'todo': 0}:
        raise ValueError('The full 1170 Node cases must pass with the original 33 skips; no relabeling')
    return counts


def validate_components(campaign, component, expected=EXPECTED):
    namespace = component.plain(campaign / expected['component'], True)
    receipt_path = namespace / 'component-receipt.json'
    receipt = component.read_json(receipt_path)
    if sha(receipt_path) != expected['receipt_sha256']:
        raise ValueError('Original component receipt changed')
    if (receipt.get('schema') != 1 or receipt.get('result') != 'failed'
        or receipt.get('qualification_complete') is not False
        or receipt.get('all_owned_command_descendants_settled') is not True
        or receipt.get('namespace') != str(namespace) or receipt.get('source') != str(campaign / 'source')
        or receipt.get('runner_sha256') != expected['runner_sha256']
        or receipt.get('original_batch_receipt') != str(campaign / 'evidence/batch-receipt.json')
        or receipt.get('original_batch_sha256') != component.APPROVED['prior_sha256']
        or receipt.get('original_batch_result') != 'failed'
        or receipt.get('previous_native_qualification', {}).get('native_source') != component.APPROVED['native']
        or receipt.get('previous_native_qualification', {}).get('native_tests') != 393):
        raise ValueError('Borrowed component/native scope crossed or is uncertain')
    rows = receipt.get('commands', [])
    if [row.get('stage') for row in rows] != component.REQUIRED_GATES or rows[0].get('result') != 'passed':
        raise ValueError('Original component stage roster changed')
    frozen = {'native': component.APPROVED['native'], 'source_roster_sha256': component.APPROVED['source_roster_sha256'],
              'public_models_roster_sha256': component.APPROVED['models_roster_sha256']}
    for position, row in enumerate(rows[1:], 1):
        passed = position < 4
        if (row.get('result') != ('passed' if passed else 'failed')
            or row.get('exit_code') != (0 if passed else 1)
            or row.get('owned_command_started') is not True or row.get('owned_descendants_settled') is not True
            or row.get('timed_out') is not False or row.get('log_oversized') is not False
            or row.get('frozen_inputs_before') != frozen
            or passed and row.get('frozen_inputs_after') != frozen):
            raise ValueError('Prior dependency/compile/Python/Node observation is incomplete')
    for name, digest in expected['logs'].items():
        if sha(component.plain(namespace / name, False)) != digest:
            raise ValueError('Original component log changed: ' + name)
    python = component.python_counts((namespace / 'full-python-contracts.log').read_text(encoding='utf-8'))
    if python != {'selected': 919, 'skipped': 19, 'executed': 900} or rows[3].get('test_counts') != python:
        raise ValueError('Actual prior full Python 919 qualification is missing')
    failed = node_summary((namespace / 'full-mocked-extension-tests.log').read_text(encoding='utf-8'))
    if failed != {'tests': 1170, 'pass': 1113, 'fail': 24, 'cancelled': 0, 'skipped': 33, 'todo': 0}:
        raise ValueError('Original complete failed Node scope changed')
    compiled = receipt.get('compiled_extension_inputs', {})
    if (compiled.get('sha256') != expected['compiled_sha256']
        or len(compiled.get('files', [])) != expected['compiled_files']
        or len(compiled.get('test_files', [])) != expected['compiled_test_files']
        or component.compiled_inputs(campaign / 'source') != compiled
        or rows[4].get('argv') != ['node', '--test', '--test-concurrency=2', *compiled['test_files']]):
        raise ValueError('Previously compiled JavaScript/complete test roster changed; do not reuse it')
    return receipt


def run_node(source, namespace, fixture, component, borrowed, owner, environment, verify, save,
             observe_root=observe_short_root):
    """One dispatch after all ownership/input/address checks; never repeats npm/Python/Cargo."""
    proof = observe_root(fixture)
    verify()
    argv = borrowed['commands'][4]['argv']
    if argv != ['node', '--test', '--test-concurrency=2', *borrowed['compiled_extension_inputs']['test_files']]:
        raise ValueError('Node-only selector crossed its complete compiled roster')
    environment = dict(environment)
    environment.update(TMPDIR=str(fixture), TMP=str(fixture), TEMP=str(fixture), AIRK_PRIVATE_CENSUS_BASE=str(fixture))
    row = {'stage': 'full-mocked-extension-tests', 'argv': argv, 'cwd': str(source), 'result': 'running',
           'fixture_root': str(fixture), 'socket_preflight': proof, 'owned_command_started': False,
           'stop_condition': 'first command failure, input change, root change, uncertain descendants or count mismatch'}
    save(row)
    started = time.monotonic()
    try:
        row['owned_command_started'] = True
        row.update(owner.run_owned(argv, source, namespace / 'full-mocked-extension-tests.log', 1800, environment))
        if not owner.successful(row):
            raise ValueError('Node command failed or descendants unsettled; retain the entire result')
        observe_root(fixture)
        verify()
        row['test_counts'] = require_node_pass((namespace / 'full-mocked-extension-tests.log').read_text(encoding='utf-8'))
        row['result'] = 'passed'
    except BaseException as error:
        row.update(result='failed', error=str(error)[:4096])
        raise
    finally:
        row['elapsed_seconds'] = round(time.monotonic() - started, 3)
        save(row)
    return row


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--campaign', type=Path, required=True)
    parser.add_argument('--namespace', required=True)
    args = parser.parse_args(argv)
    if (sys.flags.optimize or platform.system() != 'Darwin' or platform.machine() != 'arm64'
        or os.getuid() != 501 or socket.gethostname() != 'Tylers-MacBook-Neo.local'):
        raise ValueError('Only nonoptimized established UID501 Neo ARM is allowed')
    campaign = args.campaign.absolute()
    if campaign.resolve(strict=True) != campaign or campaign.parent != ROOT or campaign.name != CAMPAIGN:
        raise ValueError('Crossed original frozen Neo campaign')
    if any(path.is_symlink() for path in (campaign, *campaign.parents)):
        raise ValueError('Redirected original campaign')
    component = load_verified(component_helper_path(campaign), EXPECTED['runner_sha256'], 'verified_r36_components')
    prior = component.validate_prior(campaign)
    component.validate_frozen(campaign, prior, lambda: component.APPROVED['native'])
    builder = component.load(campaign / 'source', 'r36_node_builder', 'build-service-runtime.py')
    owner = component.load(campaign / 'source', 'r36_node_command_owner', 'provision-static-onnx-runtime.py')
    runner = component.plain(Path(__file__), False)
    runner_digest = sha(runner)

    def verify():
        helper = component_helper_path(campaign)
        component.plain(helper, False)
        if sha(helper) != EXPECTED['runner_sha256']:
            raise ValueError('Original reviewed component helper changed')
        component.validate_prior(campaign)
        component.validate_frozen(campaign, prior, builder.source_identity)
        borrowed = validate_components(campaign, component)
        if sha(runner) != runner_digest:
            raise ValueError('Reviewed Node-only runner changed')
        return borrowed

    borrowed = verify()
    # Fail before allocating any socket fixture if the frozen factory changed.
    factory = (campaign / 'source/packages/vscode/src/explorerAssistanceHost.ts').read_text(encoding='utf-8')
    if ("fs.mkdtempSync(path.join(os.tmpdir(), 'airk-explorer-'))" not in factory
        or "path.join(socketDirectory, 'airk-explorer.sock')" not in factory):
        raise ValueError('Frozen Assistance socket factory requires a fresh budget review')
    if not re.fullmatch(r'node-followup-r36-[0-9]+', args.namespace):
        raise ValueError('Invalid fresh Node-only evidence namespace')
    namespace = campaign / args.namespace
    namespace.mkdir(mode=0o700, exist_ok=False)
    component.plain(namespace, True)
    fixture = create_short_root()
    state = {'schema': 1, 'purpose': 'only remaining full mocked Node suite on original frozen Neo source',
             'result': 'running', 'qualification_complete': False, 'source': str(campaign / 'source'),
             'namespace': str(namespace), 'fixture_root': str(fixture), 'fixture_root_retained': True,
             'runner_sha256': runner_digest, 'borrowed_component_receipt_sha256': EXPECTED['receipt_sha256'],
             'borrowed_native_batch_sha256': component.APPROVED['prior_sha256'],
             'borrowed_scope': 'original Clippy/native393 and dependency/compile/Python919; no reruns/relabeling',
             'commands': [], 'limits': ['No npm, compile, Python suite, Cargo, native install, editor or release qualification rerun']}

    def save(row=None):
        if row is not None:
            state['commands'] = [row]
        (namespace / 'node-receipt.json').write_text(json.dumps(state, indent=2) + '\n')

    try:
        environment = {key: value for key, value in os.environ.items()
            if not key.startswith(('AIRK_', 'GIT_', 'PYTHON')) and key != 'CARGO_TARGET_DIR'}
        environment['PATH'] = ':'.join([str(component.TOOLS / 'node-v24.15.0-darwin-arm64/bin'),
                                       str(component.TOOLS), environment.get('PATH', '')])
        import shutil
        selected = shutil.which('node', path=environment['PATH'])
        if not selected or not component.plain(Path(selected).resolve(strict=True), False).is_relative_to(component.TOOLS / 'node-v24.15.0-darwin-arm64'):
            raise ValueError('Established Node24 prerequisite unavailable')
        environment.update(AIRK_BUNDLED_MODEL_DIR=str(campaign / 'offline-model'),
                           AIRK_TEST_MODEL_DIR=str(campaign / 'offline-model/minilm-l6-v2-s8-v1'))
        row = run_node(campaign / 'source', namespace, fixture, component, borrowed, owner, environment, verify, save)
        state['result'] = row['result']
    except BaseException as error:
        state.update(result='failed', error={'type': type(error).__name__, 'message': str(error)[:4096]})
    finally:
        state['all_owned_command_descendants_settled'] = all(row.get('owned_command_started') is False
            or row.get('owned_descendants_settled') is True for row in state['commands'])
        state['qualification_complete'] = bool(state['result'] == 'passed' and len(state['commands']) == 1
            and state['all_owned_command_descendants_settled'])
        save()
        (namespace / 'phase.txt').write_text('complete-' + state['result'])
        print(json.dumps({'result': state['result'], 'receipt': str(namespace / 'node-receipt.json')}))
    return 0 if state['qualification_complete'] else 1


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except Exception as error:
        print(type(error).__name__ + ': ' + str(error)[:4096], file=sys.stderr)
        raise SystemExit(1)
