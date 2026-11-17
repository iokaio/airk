# SPDX-License-Identifier: Apache-2.0
"""Execute an explicitly authorized frozen Core/Store snapshot on neo only."""
import hashlib
import json
import os
import pathlib
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import time

archive, manifest_path, destination = map(pathlib.Path, sys.argv[1:4])
options = set(sys.argv[4:])
assert options <= {'--storage-only', '--resume-owned-preflight'}
storage_only = '--storage-only' in options
resume_preflight = '--resume-owned-preflight' in options
assert not resume_preflight or storage_only
run = pathlib.Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh')
previous = pathlib.Path('/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P')
assert platform.system() == 'Darwin' and platform.machine() == 'arm64'
assert destination.parent.resolve() == run.resolve() and destination.name.startswith('p6f-focused-')
if resume_preflight:
    assert destination.is_dir() and not destination.is_symlink()
    assert not (destination / 'qualification.json').exists()
    assert (destination / 'clean-owned-workspace-cache.log').is_file()
else:
    assert not destination.exists()
record = json.loads(manifest_path.read_text())
assert hashlib.file_digest(archive.open('rb'), 'sha256').hexdigest() == record['archive_sha256']
if not resume_preflight:
    destination.mkdir(mode=0o700)
source = destination / 'source'
if not resume_preflight:
    source.mkdir()
    with tarfile.open(archive) as stream:
        stream.extractall(source, filter='data')
def verify_source():
    actual = {str(path.relative_to(source)): hashlib.file_digest(path.open('rb'), 'sha256').hexdigest() for path in source.rglob('*') if path.is_file() and not path.is_relative_to(source / 'target')}
    assert actual == record['files'], 'snapshot manifest changed'
    return {'files': len(actual), 'unchanged': True}
before = verify_source()
target = destination / 'target'
if not resume_preflight:
    subprocess.run(['/bin/cp', '-cR', str(run / 'target'), str(target)], check=True)
fixtures = destination / 'fixtures'
fixtures.mkdir(mode=0o700, exist_ok=resume_preflight)
environment = dict(os.environ)
environment.update(PATH=str(previous / 'tools') + ':' + str(pathlib.Path.home() / '.cargo/bin') + ':' + environment.get('PATH', ''), CARGO_BUILD_JOBS='1', CARGO_NET_OFFLINE='true', MACOSX_DEPLOYMENT_TARGET='13.3', CARGO_TARGET_DIR=str(target), AIRK_PRIVATE_CENSUS_BASE=str(fixtures), PYTHONUTF8='1')
steps = []
def step(name, command, minimum_tests=0):
    started = time.monotonic()
    logfile = destination / (name + '.log')
    with logfile.open('x') as output:
        completed = subprocess.run(command, cwd=source, env=environment, stdout=output, stderr=subprocess.STDOUT)
    text = logfile.read_text(errors='replace')
    summaries = re.findall(r'test result: ([^\r\n]+)', text)
    counts = re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored;', text)
    counts = dict(zip(['passed', 'failed', 'ignored'], map(int, counts[-1]))) if counts else None
    sufficient = minimum_tests == 0 or counts is not None and counts['passed'] + counts['failed'] >= minimum_tests
    item = {'name': name, 'command': command, 'exit_code': completed.returncode, 'seconds': round(time.monotonic()-started, 3), 'summaries': summaries, 'counts': counts, 'minimum_tests': minimum_tests, 'sufficient_test_count': sufficient, 'log': str(logfile), 'log_sha256': hashlib.file_digest(logfile.open('rb'), 'sha256').hexdigest()}
    steps.append(item)
    print(json.dumps(item), flush=True)
    return completed.returncode, text
assert not target.is_symlink() and target.resolve() == destination.resolve() / 'target'
assert target.stat().st_uid == os.getuid()
cache_identity = {}
if resume_preflight:
    for relative in ['debug/.fingerprint', 'debug/deps', 'debug/build']:
        path = target / relative
        assert path.is_dir() and not path.is_symlink() and path.resolve().is_relative_to(target.resolve())
        cache_identity[relative] = {'device': path.stat().st_dev, 'inode': path.stat().st_ino, 'owner': path.stat().st_uid}
    marker = target / 'CACHEDIR.TAG'
    with marker.open('x') as output:
        output.write('Signature: 8a477f597d28d172789f06886806bc55\n# Owned AIRK focused test cache; copied Cargo artifacts.\n# https://bford.info/cachedir/\n')
    cache_identity['absolute_owned_target'] = str(target.resolve())
    cache_identity['canonical_marker_sha256'] = hashlib.file_digest(marker.open('rb'), 'sha256').hexdigest()
    cache_identity['prior_preflight_failure_sha256'] = hashlib.file_digest((destination / 'clean-owned-workspace-cache.log').open('rb'), 'sha256').hexdigest()
    (destination / 'cache-preflight-r2.json').write_text(json.dumps(cache_identity, indent=2)+'\n')
clean_code, _ = step('clean-owned-workspace-cache-r2' if resume_preflight else 'clean-owned-workspace-cache', ['cargo', 'clean', '-p', 'airk-core', '-p', 'airk-store', '--offline', '--target-dir', str(target)])
assert clean_code == 0, 'owned workspace output cleanup failed'
packages = ['-p', 'airk-store'] if storage_only else ['-p', 'airk-core', '-p', 'airk-store']
compile_code, compile_output = step('compile', ['cargo', 'test', *packages, '--lib', '--locked', '--offline', '--no-run', '--message-format=json'])
if compile_code == 0:
    binaries = {}
    freshly_compiled = set()
    for line in compile_output.splitlines():
        try:
            row = json.loads(line)
        except ValueError:
            continue
        if row.get('reason') == 'compiler-artifact' and row.get('executable') and row.get('profile', {}).get('test'):
            binaries[row['target']['name']] = row['executable']
        if row.get('reason') == 'compiler-artifact' and row.get('target', {}).get('name') in {'airk_core', 'airk_store'} and row.get('fresh') is False:
            freshly_compiled.add(row['target']['name'])
    assert freshly_compiled == {'airk_core', 'airk_store'}, 'selected workspace output was unexpectedly reused: '+repr(freshly_compiled)
    assert set(binaries) >= ({'airk_store'} if storage_only else {'airk_core', 'airk_store'}), binaries
    if storage_only:
        list_code, listed = step('test-list', [binaries['airk_store'], '--list'])
        assert list_code == 0
        selected_counts = {'initial_service_claim': sum(line.startswith('quota::initial_service_claim_') and line.endswith(': test') for line in listed.splitlines()), 'storage_setup': sum(line.startswith('service::registry::catalog::setup::storage::tests::') and line.endswith(': test') for line in listed.splitlines())}
        assert selected_counts == {'initial_service_claim': 2, 'storage_setup': 9}, 'unexpected selected test catalog: '+repr(selected_counts)
        steps[-1]['selected_case_counts'] = selected_counts
    if not storage_only:
        step('core', [binaries['airk_core'], '--test-threads=1'], 1)
        step('quota-setup', [binaries['airk_store'], 'quota::setup::tests::', '--test-threads=1', '--nocapture'], 10)
    if storage_only:
        step('initial-service-claim', [binaries['airk_store'], 'quota::initial_service_claim_', '--test-threads=1', '--nocapture'], 2)
    step('storage-setup', [binaries['airk_store'], 'service::registry::catalog::setup::storage::tests::', '--test-threads=1', '--nocapture'], 9 if storage_only else 8)
    binary_hashes = {name: hashlib.file_digest(pathlib.Path(path).open('rb'), 'sha256').hexdigest() for name, path in binaries.items()}
else:
    binary_hashes = {}
after = verify_source()
result = {'schema': 'airk.private-focused-native-qualification/1', 'scope': 'Selected Store Unix storage setup and initial quota claim tests only' if storage_only else record['scope'], 'platform': platform.platform(), 'native_source': record['native_source'], 'component_source': record.get('component_source'), 'source_manifest_sha256': hashlib.file_digest(manifest_path.open('rb'), 'sha256').hexdigest(), 'source_before': before, 'source_after': after, 'steps': steps, 'binary_sha256': binary_hashes, 'fresh_workspace_compilation': sorted(freshly_compiled) if compile_code == 0 else [], 'owned_clone_only_cache_cleanup': clean_code == 0, 'status': 'passed' if compile_code == 0 and all(item['exit_code'] == 0 and item['sufficient_test_count'] for item in steps) else 'failed', 'not_run': ['full workspace tests', 'full P6 campaign', 'release build', 'model qualification', 'extension/editor tests', 'signing/notarization', 'Linux/GNU Unix qualification'] + (['Core library test execution', 'quota setup test execution'] if storage_only else []), 'user_state_changed': False, 'retained_p6e_control_changed': False}
(destination / 'qualification.json').write_text(json.dumps(result, indent=2)+'\n')
print(json.dumps(result, indent=2), flush=True)
