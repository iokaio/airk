# SPDX-License-Identifier: Apache-2.0
"""Root-owned final local Windows queue. No normal install, publication or retries.

Run only after the prior Cargo owner and compiler descendants have exited and
the final cli_commands test correction is applied. A failed stage stops the
queue; native fixtures retain their own failure/cleanup evidence. Root resolves
any unknown owned processes before handing Cargo to another owner.
"""
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

BASE = Path(__file__).resolve().parent
WORK = BASE / 'p10-worktree'
TARGET = BASE / 'p10-target'
PLATFORM = 'x86_64-pc-windows-msvc'


def sha(path):
    with Path(path).open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def read(path):
    return json.loads(Path(path).read_text(encoding='utf-8-sig'))


def save_new(path, value):
    with Path(path).open('x', encoding='utf-8') as output:
        json.dump(value, output, indent=2)
        output.write('\n')


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def files(directory):
    result = {}
    for path in sorted(directory.rglob('*')):
        assert not path.is_symlink() and not path.is_junction(), str(path)
        if path.is_file():
            result[path.relative_to(directory).as_posix()] = sha(path)
    return result


def public_inputs():
    names = subprocess.check_output(['git', '-c', f'safe.directory={WORK.as_posix()}',
        'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=WORK).split(b'\0')
    omitted = {'.git', '.codex', '.agents', '.aws', '.mcp.json', 'temp', 'target', 'artifacts'}
    result = {}
    for name in sorted(set(names)):
        if not name or os.fsdecode(name).split('/')[0] in omitted:
            continue
        path = WORK / os.fsdecode(name)
        assert not path.is_symlink() and not path.is_junction() and path.resolve().is_relative_to(WORK)
        result[os.fsdecode(name).replace('\\', '/')] = sha(path) if path.is_file() else None
    return result


def compile_inputs(public):
    prefix = 'packages/vscode/'
    return {name: digest for name, digest in public.items() if name.startswith(prefix)
        and (name.startswith(prefix + 'src/') or name.startswith(prefix + 'scripts/')
             or name.startswith(prefix + 'tsconfig')
             or name in (prefix + 'package.json', prefix + 'package-lock.json'))}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--label', required=True)
    parser.add_argument('--source-sha256', required=True)
    parser.add_argument('--source-files', type=int, required=True)
    parser.add_argument('--compiled-proof', type=Path,
        help='Optional actual canonical compile receipt plus exact input/output maps; otherwise compile once')
    args = parser.parse_args()
    assert os.name == 'nt' and re.fullmatch(r'[a-z0-9][a-z0-9-]{0,79}', args.label)
    assert re.fullmatch(r'[0-9a-f]{64}', args.source_sha256) and args.source_files > 0
    expected = {'sha256': args.source_sha256, 'files': args.source_files}
    assert WORK.is_dir() and TARGET.is_dir()
    output = BASE / args.label
    assert not output.exists() and not output.is_symlink()
    output.mkdir()
    # These outputs must be under WORK/artifacts for the canonical packagers.
    package_base = WORK / 'artifacts' / args.label
    assert not package_base.exists() and not package_base.is_symlink()
    package_base.mkdir()
    runtime = package_base / 'runtime-release'
    native = package_base / 'native'
    extension = package_base / 'extension'
    compiled = WORK / 'packages/vscode/out'
    lock = BASE / 'post-p14-windows-closeout-cargo-owner.lock'
    descriptor = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
    os.write(descriptor, str(os.getpid()).encode('ascii'))
    builder = load('closeout_builder', WORK / 'scripts/build-service-runtime.py')
    packager = load('closeout_packager', WORK / 'scripts/package-native.py')
    record = {'status': 'running', 'source_expected': expected, 'steps': [],
        'output': str(output), 'package_output': str(package_base),
        'target': str(TARGET), 'version': '1.8.8',
        'scope': 'Local Windows artifacts and disposable native fixtures only; no normal installation or publication',
        'cargo_ownership': 'Root must establish prior compiler quiescence before launch; sequential child completion within this queue'}
    environment = os.environ.copy()
    for key in ('AIRK_TEST_BINARY', 'AIRK_CATALOG_CANDIDATE_BINARY',
                'AIRK_P10_RESOURCE_OUTPUT_DIR', 'AIRK_P11B_RESOURCE_OUTPUT_DIR'):
        environment.pop(key, None)
    mapping_path = BASE / 'p9-clean-inputs.local.json'
    mapping = read(mapping_path)['inputs']
    environment.update(CARGO_HOME='C:/Users/tdotn/.cargo', RUSTUP_HOME='C:/Users/tdotn/.rustup',
        CARGO_TARGET_DIR=str(TARGET), CARGO_NET_OFFLINE='true', PYTHONUTF8='1',
        PYTHONDONTWRITEBYTECODE='1', AIRK_BUNDLED_MODEL_DIR=mapping['local-vector-model'],
        AIRK_BUNDLED_RUNTIME_DIR=mapping['local-onnx-runtime'],
        AIRK_TEST_MODEL_DIR=str(Path(mapping['local-vector-model']) / 'minilm-l6-v2-u8-v1'))
    assert all(Path(environment[key]).is_dir() for key in
        ('AIRK_BUNDLED_MODEL_DIR', 'AIRK_BUNDLED_RUNTIME_DIR', 'AIRK_TEST_MODEL_DIR'))
    environment['PATH'] = 'C:/Users/tdotn/.cargo/bin;' + environment.get('PATH', '')
    environment['GIT_CONFIG_GLOBAL'] = str(BASE / 'p10-owned-ignore-controls/global.gitconfig')
    count = int(environment.get('GIT_CONFIG_COUNT', '0'))
    environment[f'GIT_CONFIG_KEY_{count}'] = 'safe.directory'
    environment[f'GIT_CONFIG_VALUE_{count}'] = WORK.as_posix()
    environment['GIT_CONFIG_COUNT'] = str(count + 1)
    proof_catalog = BASE / 'p14-final-qualification-command-map-r4.json'
    private = read(proof_catalog)['private_base_proof']
    environment['AIRK_PRIVATE_CENSUS_BASE'] = private['path']
    # Imported builder subprocesses inherit this same scoped environment.
    os.environ.update(environment)
    public = public_inputs()
    inputs = compile_inputs(public)
    pins = {str(p): sha(p) for p in (mapping_path, proof_catalog,
        Path(private['receipt']), Path(private['verifier']),
        Path(environment['GIT_CONFIG_GLOBAL']), Path(__file__).resolve())}
    record.update(public_inputs=public, pinned_owner_inputs=pins)
    seal = None
    outputs = None

    def checkpoint():
        assert builder.source_identity() == expected, 'Native source changed'
        assert public_inputs() == public, 'Public source changed'
        assert all(sha(Path(name)) == digest for name, digest in pins.items()), 'Owner input changed'
        assert sha(Path(private['receipt'])) == private['receipt_sha256']
        result = subprocess.run(['pwsh', '-NoProfile', '-File', private['verifier'],
            '-ExpectedReceiptSha256', private['receipt_sha256']], capture_output=True, text=True,
            encoding='utf-8', env=environment)
        assert result.returncode == 0, result.stdout + result.stderr
        value = json.loads(result.stdout.lstrip('\ufeff'))
        assert value['acl_mutated'] is False and all(value[key] == private[key]
            for key in ('path', 'owner_sid', 'sddl'))
        if seal is not None:
            assert packager.validate_runtime(runtime, '1.8.8', PLATFORM, expected)[0] == seal
        if outputs is not None:
            assert files(compiled) == outputs, 'Compiled extension changed'

    def timings():
        directory = TARGET / 'cargo-timings'
        return {path.name: sha(path) for path in directory.glob('*.html')}

    def run(name, command, incremental='1', native_binary=False):
        checkpoint()
        before = builder.source_identity()
        previous = timings()
        env = environment.copy()
        env['CARGO_INCREMENTAL'] = incremental
        if native_binary:
            # Set INSIDE this owner: p11-local-command strips inherited values.
            env['AIRK_TEST_BINARY'] = str(runtime / 'airk.exe')
            assert sha(runtime / 'airk.exe') == seal['airk.exe']
        log = output / (name + '.log')
        print('START', name, flush=True)
        start = time.monotonic()
        failure = None
        code = None
        try:
            with log.open('x', encoding='utf-8') as stream:
                code = subprocess.run(command, cwd=WORK, env=env, stdout=stream,
                    stderr=subprocess.STDOUT, creationflags=subprocess.CREATE_NO_WINDOW).returncode
        except BaseException as error:
            failure = repr(error)
        elapsed = time.monotonic() - start
        captured = {}
        for filename, digest in timings().items():
            if previous.get(filename) != digest:
                destination = output / (name + '-' + filename)
                with destination.open('xb') as stream:
                    stream.write((TARGET / 'cargo-timings' / filename).read_bytes())
                assert sha(destination) == digest
                captured[str(destination)] = digest
        try:
            checkpoint()
        except BaseException as error:
            failure = failure or repr(error)
        row = {'name': name, 'command': command, 'cwd': str(WORK),
            'cargo_incremental': incremental, 'target': str(TARGET),
            'AIRK_TEST_BINARY': env.get('AIRK_TEST_BINARY'), 'exit_code': code,
            'source_before': before, 'source_after': builder.source_identity(),
            'seconds': elapsed, 'log_sha256': sha(log) if log.exists() else None,
            'timings': captured, 'validation_error': failure}
        save_new(output / (name + '.receipt.json'), row)
        record['steps'].append(row)
        assert code == 0 and failure is None, name + ': ' + str(failure or code)
        return log

    try:
        checkpoint()
        py = sys.executable
        run('lightweight-main', ['cargo', 'check', '-p', 'airk-cli', '--no-default-features',
            '--all-targets', '--locked', '--offline', '--timings'])
        run('workspace-build', ['cargo', 'build', '--workspace', '--locked', '--offline', '--timings'])
        run('workspace-doctests', ['cargo', 'test', '--doc', '--workspace', '--all-features',
            '--locked', '--offline', '--timings', '--', '--test-threads=1'])
        run('optimized-pair', [py, 'scripts/build-service-runtime.py', '--profile', 'release',
            '--destination', str(runtime)], incremental='0')
        seal = packager.validate_runtime(runtime, '1.8.8', PLATFORM, expected)[0]
        save_new(output / 'runtime-seal.json', {'source': expected, 'files': seal,
            'install_manifest_sha256': seal['airk-install.json'], 'profile': 'release'})
        if args.compiled_proof:
            proof = read(args.compiled_proof)
            receipt = read(proof['receipt'])
            assert sha(Path(proof['receipt'])) == proof['receipt_sha256']
            assert receipt['actual_exit_code'] == receipt['exit_code'] == 0
            assert receipt.get('validation_error') is None
            command = receipt['command']
            assert Path(command[0]).stem in ('npm', 'npm.cmd')
            assert command[1:4] == ['run', 'compile', '--prefix'] and len(command) == 5
            assert Path(command[4]).resolve() == WORK / 'packages/vscode'
            assert proof['inputs'] == inputs and proof['outputs'] == files(compiled)
            log = Path(proof['receipt']).with_name(Path(proof['receipt']).name.replace('.receipt.json', '.log'))
            assert sha(log) == receipt['log_sha256']
            pins.update({str(args.compiled_proof.resolve()): sha(args.compiled_proof),
                str(Path(proof['receipt']).resolve()): proof['receipt_sha256'], str(log.resolve()): sha(log)})
            record['compiled_reuse'] = {'proof': str(args.compiled_proof), 'executed': False}
        else:
            run('extension-compile', ['npm.cmd', 'run', 'compile', '--prefix', str(WORK / 'packages/vscode')])
        outputs = files(compiled)
        assert outputs and files(WORK / 'packages/vscode/out/test')
        save_new(output / 'compiled-proof.json', {'inputs': inputs, 'outputs': outputs,
            'basis': record.get('compiled_reuse', {'executed': True, 'receipt': str(output / 'extension-compile.receipt.json')})})
        run('native-package', [py, 'scripts/package-native.py', '--target', PLATFORM,
            '--runtime-dir', str(runtime), '--output', str(native)])
        run('vsix-package', [py, 'scripts/package-extension.py', '--target', PLATFORM,
            '--native-dir', str(native / 'publish'), '--output', str(extension)])
        node_tests = sorted((compiled / 'test').glob('*.test.js'))
        assert node_tests and all(path.is_file() for path in node_tests)
        save_new(output / 'node-test-inventory.json', {str(path.relative_to(compiled)): sha(path) for path in node_tests})
        node_log = run('extension-all-node', ['node', '--test', '--test-reporter=tap',
            '--test-concurrency=2', *map(str, node_tests)], native_binary=True)
        text = node_log.read_text(encoding='utf-8', errors='strict')
        counts = {key: int(number) for key, number in re.findall(r'^# (tests|pass|fail|cancelled|skipped) (\d+)\s*$', text, re.M)}
        assert counts.get('tests', 0) > 0 and counts.get('pass', 0) > 0
        assert counts.get('fail') == 0 and counts.get('cancelled') == 0
        record['node_counts'] = counts
        report = output / 'clients-16-model.json'
        run('clients-16-model', [py, 'scripts/check-service-clients.py', '--runtime-dir', str(runtime),
            '--files', '16', '--model', '--report', str(report)])
        assert report.is_file()
        run('owned-install-fixture', [py, 'scripts/check-service-install.py', '--runtime', str(runtime)])
        native_manifest = read(native / 'publish/manifest.json')
        vsix_manifest = read(extension / 'publish/manifest.json')
        assert native_manifest['version'] == vsix_manifest['version'] == '1.8.8'
        assert native_manifest['source'] == expected
        assert native_manifest['install_manifest_sha256'] == seal['airk-install.json']
        assert vsix_manifest['native_manifest_sha256'] == sha(native / 'publish/manifest.json')
        assert vsix_manifest['native_archive_sha256'] == native_manifest['archive_sha256']
        for directory, manifest in ((native / 'publish', native_manifest), (extension / 'publish', vsix_manifest)):
            assert sha(directory / manifest['archive']) == manifest['archive_sha256']
        checkpoint()
        record.update(status='passed', runtime_seal=seal,
            native_manifest_sha256=sha(native / 'publish/manifest.json'),
            vsix_manifest_sha256=sha(extension / 'publish/manifest.json'),
            vsix=str(extension / 'publish' / vsix_manifest['archive']),
            vsix_sha256=vsix_manifest['archive_sha256'], clients_report_sha256=sha(report))
    except BaseException as error:
        record.update(status='failed', error=repr(error),
            cleanup='No normal installation or Windows target clean. Retain failed fixtures; Root must inspect any unknown owned processes.')
        raise
    finally:
        save_new(output / 'closeout.json', record)
        os.close(descriptor)
        lock.unlink()
    return 0


if __name__ == '__main__':
    sys.exit(main())
