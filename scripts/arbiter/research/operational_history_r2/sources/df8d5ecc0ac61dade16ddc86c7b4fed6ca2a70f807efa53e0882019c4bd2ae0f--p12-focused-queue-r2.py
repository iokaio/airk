# SPDX-License-Identifier: Apache-2.0
"""Explicitly launched Windows LOCAL P12 focused queue. No release producer."""
import argparse
import atexit
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import xml.etree.ElementTree as ET

BASE = Path(__file__).resolve().parent
WORK = BASE / 'p10-worktree'
TARGET = BASE / 'p10-target'
PROPOSAL = BASE / 'p12-focused-selection-r2'
WRAPPER = BASE / 'p11-local-command.py'
PACKAGES = ['airk-core', 'airk-ingest', 'airk-store', 'airk-kernel', 'airk-mcp', 'airk-cli']


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def native_identity(env):
    # Same framed input roster as the existing build-service-runtime.py owner.
    paths = ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo', 'crates',
             'scripts/build-service-runtime.py', 'scripts/provision-onnx-runtime.py',
             'scripts/provision-local-model.py', 'scripts/int8-policy.py',
             'scripts/macho-identity.py', 'scripts/license-notices.py', 'LICENSE',
             'NOTICE', 'THIRD_PARTY_NOTICES.md', 'third_party']
    names = subprocess.check_output(['git', '-c', 'safe.directory=' + WORK.as_posix(),
             'ls-files', '-z', '--cached', '--others', '--exclude-standard', '--', *paths],
             cwd=WORK, env=env)
    names = sorted(set(name for name in names.split(b'\0') if name))
    value = hashlib.sha256()
    for name in names:
        path = WORK / os.fsdecode(name)
        assert not path.is_symlink() and path.resolve().is_relative_to(WORK.resolve())
        value.update(len(name).to_bytes(8, 'big'))
        value.update(name)
        value.update(bytes.fromhex(digest(path)) if path.is_file() else b'missing')
    return {'sha256': value.hexdigest(), 'files': len(names)}


def same_clippy_union(command):
    if len(command) < 3 or Path(command[0]).stem != 'cargo' or command[1] != 'clippy':
        return False
    try:
        boundary = command.index('--')
        if command[boundary + 1:] != ['-D', 'warnings']:
            return False
        packages, flags = [], []
        index = 2
        while index < boundary:
            token = command[index]
            if token in ['-p', '--package']:
                packages.append(command[index + 1])
                index += 2
            else:
                flags.append(token)
                index += 1
        if flags.count('--keep-going') > 1:
            return False
        flags = [flag for flag in flags if flag != '--keep-going']
        return sorted(packages) == sorted(PACKAGES) and sorted(flags) == sorted([
            '--locked', '--offline', '--all-targets', '--timings', '--all-features'])
    except (ValueError, IndexError):
        return False


def junit_state(path):
    if not path.exists():
        return None
    return (path.stat().st_mtime_ns, path.stat().st_size, digest(path))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--label', required=True, help='Fresh run label, for example r1')
    parser.add_argument('--clippy-receipt', type=Path, help='Adopt completed exact-source scoped Clippy; failure never triggers a rerun')
    parser.add_argument('--source-sha256', required=True, help='Explicit parent-approved AFTER-APPLY, AFTER-FORMAT freeze')
    parser.add_argument('--source-files', required=True, type=int)
    args = parser.parse_args()
    assert os.name == 'nt', 'Existing local wrapper is Windows-specific; use the separate Neo proposal'
    assert re.fullmatch(r'r[1-9][0-9]*', args.label)
    assert re.fullmatch(r'[0-9a-f]{64}', args.source_sha256) and args.source_files > 948
    expected = {'sha256': args.source_sha256, 'files': args.source_files}
    output = BASE / ('p12-focused-' + args.label)
    assert not output.exists(), 'Never overwrite a previous result; collect it and choose a fresh label'
    conflicts = list(BASE.glob('*qualification-owner*.lock'))
    assert not conflicts, 'Another qualification owner exists: ' + ', '.join(map(str, conflicts))
    lock = BASE / 'p12-focused-owner.lock'
    owner_fd = os.open(lock, os.O_WRONLY | os.O_CREAT | os.O_EXCL)
    released = False

    def release_owner():
        nonlocal released
        if released:
            return
        released = True
        original = os.fstat(owner_fd)
        os.close(owner_fd)
        # Remove only this process's exact owned lock, including early setup errors.
        if lock.exists():
            current = lock.stat()
            if (current.st_dev, current.st_ino) == (original.st_dev, original.st_ino):
                lock.unlink()

    atexit.register(release_owner)
    os.write(owner_fd, str(os.getpid()).encode())
    output.mkdir()
    pins = {str(p): digest(p) for p in [WRAPPER, PROPOSAL / 'focused-filter.txt',
             PROPOSAL / 'added-tests.json', PROPOSAL / 'baseline-tests.json', BASE / 'p12-native-composed-p11b-r2/p12-native-composed.patch',
             WORK / '.config/nextest.toml', WORK / 'scripts/build-service-runtime.py',
             BASE / 'p10-private-base-r5.json', BASE / 'p10-verify-private-base-r5.ps1']}
    assert pins[str(BASE / 'p12-native-composed-p11b-r2/p12-native-composed.patch')] == '9fc4fe4a58f5fb5002c945bb3bd7400fae5b10526e1f23e9c9540a342f59308b'
    catalog = json.loads((PROPOSAL / 'added-tests.json').read_text(encoding='utf-8'))
    required = [row['test'] for row in catalog]
    assert len(required) == len(set(required)) == 18
    baselines = [row['test'] for row in json.loads((PROPOSAL / 'baseline-tests.json').read_text(encoding='utf-8'))]
    assert len(baselines) == len(set(baselines)) == 21
    selection = (PROPOSAL / 'focused-filter.txt').read_text(encoding='utf-8').strip()
    private = json.loads((BASE / 'p10-private-base-r5.json').read_text(encoding='utf-8'))
    env = os.environ.copy()
    env['PATH'] = str(BASE.parent / 'tools/nextest') + os.pathsep + env.get('PATH', '')
    env.update(PYTHONUTF8='1', PYTHONDONTWRITEBYTECODE='1', CARGO_NET_OFFLINE='true',
               AIRK_DEV_CARGO_INCREMENTAL='1', CARGO_INCREMENTAL='1',
               AIRK_PRIVATE_CENSUS_BASE=private['path'],
               GIT_CONFIG_GLOBAL=str(BASE / 'p10-owned-ignore-controls/global.gitconfig'))
    assert Path(env['GIT_CONFIG_GLOBAL']).is_file(), 'Use the already owned existing ignore fixture'
    # The actual integration fixture copies Cargo main/helper into its own matched
    # debug pair. Never run against an older installed/candidate image by accident.
    for key in ['AIRK_CATALOG_CANDIDATE_BINARY', 'AIRK_TEST_BINARY', 'AIRK_P10_RESOURCE_OUTPUT_DIR', 'AIRK_P11B_RESOURCE_OUTPUT_DIR']:
        env.pop(key, None)
    record = {'status': 'running', 'source_expected': expected, 'local_incremental': '1',
              'script_sha256': digest(Path(__file__)), 'pins': pins, 'packages': PACKAGES,
              'features': '--all-features', 'required_new_tests': required,
              'required_baseline_tests': baselines, 'steps': [],
              'runtime': 'Actual helper test uses Cargo DEBUG main/helper and its owned matched pair; candidate environment unset. No release qualification.'}
    result_path = output / 'qualification.json'

    def save():
        pending = result_path.with_suffix('.pending')
        pending.write_text(json.dumps(record, indent=2) + '\n', encoding='utf-8')
        pending.replace(result_path)

    def validate():
        assert native_identity(env) == expected, 'Frozen source changed or proposal not applied'
        for name, value in pins.items():
            assert digest(Path(name)) == value, 'Pinned input drift: ' + name
        check = subprocess.run(['pwsh', '-NoProfile', '-File', str(BASE / 'p10-verify-private-base-r5.ps1'),
                    '-ExpectedReceiptSha256', pins[str(BASE / 'p10-private-base-r5.json')]],
                    capture_output=True, text=True, env=env)
        assert check.returncode == 0, check.stdout + check.stderr
        observation = json.loads(check.stdout.lstrip('\ufeff'))
        assert all(observation[k] == private[k] for k in ['path', 'owner_sid', 'sddl'])
        assert observation['acl_mutated'] is False
        return observation

    flags = [item for package in PACKAGES for item in ['-p', package]]
    stages = [('fmt', ['cargo', 'fmt', '--all', '--', '--check']),
              ('nextest-version', ['cargo', 'nextest', '--version']),
              ('clippy', ['cargo', 'clippy', '--locked', '--offline', '--all-targets', '--timings', '--keep-going',
                          *flags, '--all-features', '--', '-D', 'warnings']),
              ('tests', ['cargo', 'nextest', 'run', '--profile', 'ci', '--locked', '--offline',
                         '--timings', '--all-features', *flags, '--test-threads', '2',
                         '--no-tests', 'fail', '--no-fail-fast', '--retries', '0', '-E', selection])]
    save()
    try:
        if args.clippy_receipt is not None:
            receipt_path = args.clippy_receipt.resolve(strict=True)
            assert receipt_path.is_relative_to(BASE.resolve()) and receipt_path.name.endswith('.receipt.json')
            observed = json.loads(receipt_path.read_text(encoding='utf-8'))
            log_path = receipt_path.with_name(receipt_path.name.replace('.receipt.json', '.log'))
            assert same_clippy_union(observed['command']), 'Different Clippy package/features/flags; do not adopt'
            assert observed['exit_code'] == observed['actual_exit_code'] == 0 and observed['validation_error'] is None
            assert observed['source_before'] == observed['source_after'] == expected and observed['source_unchanged']
            assert observed['target'] == str(TARGET) and observed['cargo_incremental'] == '1'
            assert digest(log_path) == observed['log_sha256']
            pins[str(receipt_path)] = digest(receipt_path)
            pins[str(log_path)] = digest(log_path)
            record['adopted_clippy'] = {'receipt': str(receipt_path), 'receipt_sha256': pins[str(receipt_path)],
                'log_sha256': observed['log_sha256'], 'executed_in_this_queue': False,
                'command': observed['command'], 'source': expected}
            stages = [(name, command) for name, command in stages if name != 'clippy']
            save()
        for name, command in stages:
            observation = validate()
            log = output / (name + '.log')
            junit = WORK / 'target/nextest/ci/junit.xml'
            old_junit = junit_state(junit) if name == 'tests' else None
            started = time.monotonic()
            print('START', name, flush=True)
            result = subprocess.run([sys.executable, str(WRAPPER), str(log), *command], cwd=WORK, env=env)
            receipt_path = log.with_suffix('.receipt.json')
            receipt = json.loads(receipt_path.read_text(encoding='utf-8'))
            step = {'name': name, 'seconds': time.monotonic() - started, 'receipt': str(receipt_path),
                    'wrapper_exit': result.returncode, 'private_base_observation': observation}
            record['steps'].append(step)
            save()
            assert receipt['command'] == command and receipt['target'] == str(TARGET)
            assert receipt['source_before'] == receipt['source_after'] == expected and receipt['source_unchanged']
            assert receipt['cargo_incremental'] == '1'
            assert digest(log) == receipt['log_sha256']
            if name == 'nextest-version':
                assert re.search(r'\b0\.9\.148\b', log.read_text(encoding='utf-8')), 'Pinned nextest version differs'
            if name == 'tests':
                # Preserve all results even when nextest reports failures. No retry.
                state = junit_state(junit)
                assert state is not None and state != old_junit, 'Fresh nextest JUnit not written'
                assert state[1] <= 16 * 1024 * 1024, 'Unexpected JUnit payload growth'
                with (output / 'junit.xml').open('xb') as target, junit.open('rb') as source:
                    target.write(source.read())
                cases = ET.parse(output / 'junit.xml').getroot().findall('.//testcase')
                inventory = [{'name': c.get('name', ''), 'classname': c.get('classname', ''),
                              'failed': c.find('failure') is not None or c.find('error') is not None,
                              'skipped': c.find('skipped') is not None} for c in cases]
                with (output / 'case-inventory.json').open('x', encoding='utf-8') as target:
                    json.dump(inventory, target, indent=2)
                step.update(junit_sha256=digest(output / 'junit.xml'), cases=len(cases), inventory=str(output / 'case-inventory.json'))
                save()
                assert len(cases) == len(required) + len(baselines) == 39, 'Exact selected case count changed'
                assert not any(c['failed'] or c['skipped'] for c in inventory), 'Focused cases failed/skipped'
                for wanted in required + baselines:
                    found = [c for c in inventory if c['name'].rsplit('::', 1)[-1] == wanted]
                    assert len(found) == 1, 'Required exact test selection missing/ambiguous: ' + wanted
                assert not any(c['name'].endswith('abrupt_partial_marker_stage_child') for c in inventory)
                # Record the real debug producer inputs as observations, not a release seal.
                step['cargo_debug_images'] = {image: digest(TARGET / 'debug' / image)
                    for image in ['airk.exe', 'airk-embed.exe']}
                step['candidate_environment_unset'] = True
            save()
            assert result.returncode == receipt['exit_code'] == receipt['actual_exit_code'] == 0, name
            validate()
        record['status'] = 'passed'
    except BaseException as error:
        record.update(status='failed', failure=str(error))
        save()
        raise
    finally:
        # Preserve output and all current run results. Never clean shared Windows cache.
        release_owner()
    save()
    print(json.dumps({'status': record['status'], 'result': str(result_path), 'required_new_tests': 18}))


if __name__ == '__main__':
    main()
