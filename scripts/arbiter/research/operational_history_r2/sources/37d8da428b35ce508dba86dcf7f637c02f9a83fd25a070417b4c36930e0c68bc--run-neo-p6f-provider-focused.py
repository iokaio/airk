# SPDX-License-Identifier: Apache-2.0
"""Focused guarded-provider campaign. Execute only after the integration owner's GO."""
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
import tomllib

sys.dont_write_bytecode = True
archive, manifest_path, destination = map(pathlib.Path, sys.argv[1:4])
delta_only = sys.argv[4:] == ['--delta-only']
assert not sys.argv[4:] or delta_only
run = pathlib.Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh')
previous = pathlib.Path('/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P')
assert platform.system() == 'Darwin' and platform.machine() == 'arm64'
assert destination.parent.resolve() == run.resolve() and destination.name.startswith('p6f-focused-provider-')
assert not destination.exists()
manifest = json.loads(manifest_path.read_text())
assert manifest['native_source'] is not None, 'provider build needs a coherent complete native snapshot'
assert hashlib.file_digest(archive.open('rb'), 'sha256').hexdigest() == manifest['archive_sha256']
destination.mkdir(mode=0o700)
source = destination / 'source'
source.mkdir()
with tarfile.open(archive) as stream:
    stream.extractall(source, filter='data')
def source_check():
    actual = {str(path.relative_to(source)): hashlib.file_digest(path.open('rb'), 'sha256').hexdigest() for path in source.rglob('*') if path.is_file() and not path.is_relative_to(source / '.git')}
    assert actual == manifest['files'], 'copied public source changed'
    return {'files': len(actual), 'unchanged': True}
before = source_check()
environment = dict(os.environ)
environment.update(PATH=str(previous / 'tools') + ':' + str(pathlib.Path.home() / '.cargo/bin') + ':' + environment.get('PATH', ''), CARGO_BUILD_JOBS='1', CARGO_NET_OFFLINE='true', MACOSX_DEPLOYMENT_TARGET='13.3', PYTHONUTF8='1', PYTHONDONTWRITEBYTECODE='1', AIRK_BUNDLED_MODEL_DIR=str(run / 'target/local-vector-model'), AIRK_BUNDLED_RUNTIME_DIR=str(run / 'target/local-onnx-runtime'))
target = destination / 'target'
subprocess.run(['/bin/cp', '-cR', str(run / 'target'), str(target)], check=True)
assert target.resolve() == destination.resolve() / 'target' and not target.is_symlink() and target.stat().st_uid == os.getuid()
for name in ['debug/.fingerprint', 'debug/deps', 'debug/build']:
    entry = target / name
    assert entry.is_dir() and not entry.is_symlink() and entry.resolve().is_relative_to(target.resolve())
tag = target / 'CACHEDIR.TAG'
if not tag.exists():
    with tag.open('x') as output:
        output.write('Signature: 8a477f597d28d172789f06886806bc55\n# Owned AIRK guarded-provider test cache.\n# https://bford.info/cachedir/\n')
else:
    assert not tag.is_symlink() and tag.read_bytes().startswith(b'Signature: 8a477f597d28d172789f06886806bc55')
environment['CARGO_TARGET_DIR'] = str(target)
fixtures = destination / 'private-fixtures'
fixtures.mkdir(mode=0o700)
environment['AIRK_PRIVATE_CENSUS_BASE'] = str(fixtures)
steps = []
def step(name, command, minimum=0, timeout=1800):
    logfile = destination / (name + '.log')
    started = time.monotonic()
    timed_out = False
    with logfile.open('x') as output:
        try:
            result = subprocess.run(command, cwd=source, env=environment, stdout=output, stderr=subprocess.STDOUT, timeout=timeout)
            code = result.returncode
        except subprocess.TimeoutExpired:
            timed_out, code = True, 124
    content = logfile.read_text(errors='replace')
    matches = re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored;', content)
    counts = dict(zip(['passed', 'failed', 'ignored'], map(int, matches[-1]))) if matches else None
    sufficient = minimum == 0 or counts is not None and counts['passed'] + counts['failed'] >= minimum
    row = {'name': name, 'command': command, 'exit_code': code, 'seconds': round(time.monotonic()-started, 3), 'timeout_seconds': timeout, 'timed_out': timed_out, 'counts': counts, 'minimum_tests': minimum, 'sufficient_test_count': sufficient, 'log': str(logfile), 'log_sha256': hashlib.file_digest(logfile.open('rb'), 'sha256').hexdigest()}
    steps.append(row)
    print(json.dumps(row), flush=True)
    return code, content
def artifacts(content):
    rows = []
    for line in content.splitlines():
        try:
            row = json.loads(line)
        except ValueError:
            continue
        if row.get('reason') == 'compiler-artifact':
            rows.append(row)
    return rows
def execute():
    clean, _ = step('clean-owned-workspace-cache', ['cargo', 'clean', '--workspace', '--offline', '--target-dir', str(target)])
    if clean:
        return
    check, check_text = step('cli-all-targets-check', ['cargo', 'check', '-p', 'airk-cli', '--all-targets', '--all-features', '--locked', '--offline', '--message-format=json'])
    if check:
        return
    native = {row['target']['name'] for row in artifacts(check_text) if row.get('fresh') is False}
    assert {'airk_cli', 'airk_core', 'airk_store'} <= native, 'expected native components were not freshly checked'
    test_packages = ['-p', 'airk-cli'] if delta_only else ['-p', 'airk-cli', '-p', 'airk-store']
    compile_code, content = step('test-compile', ['cargo', 'test', *test_packages, '--lib', '--locked', '--offline', '--no-run', '--message-format=json'])
    if compile_code:
        return
    rows = artifacts(content)
    binary = {row['target']['name']: row['executable'] for row in rows if row.get('executable') and row.get('profile', {}).get('test')}
    expected_test_binaries = {'airk_cli'} if delta_only else {'airk_cli', 'airk_store'}
    assert expected_test_binaries <= set(binary)
    assert all(any(row['target']['name'] == name and row.get('fresh') is False for row in rows) for name in expected_test_binaries), 'test artifacts were unexpectedly reused'
    if delta_only:
        name = 'embedding_pool_process::shared::tests::shared_recovery_requires_positive_coverage_and_exact_admission'
        listed, catalog = step('cli-test-catalog', [binary['airk_cli'], '--list'])
        assert listed == 0 and catalog.splitlines().count(name + ': test') == 1, 'corrective case is missing or duplicated'
        step(name.rsplit('::', 1)[-1], [binary['airk_cli'], '--exact', name, '--test-threads=1', '--nocapture'], 1, 180)
        (destination / 'delta-evidence.json').write_text(json.dumps({'source': manifest['native_source'], 'test_binary_sha256': hashlib.file_digest(pathlib.Path(binary['airk_cli']).open('rb'), 'sha256').hexdigest(), 'prior_actual_guarded_lifecycle_source': 'caf16f991de93218eaa6265f499a4f0d3d0cd4ecf5fdb1b95fe7700b9844b95a', 'prior_actual_guarded_lifecycle_repeated': False, 'pair_built': False, 'explicit_signing_invoked': False}, indent=2)+'\n')
        return
    tests = [
        'embedding_pool_process::shared::tests::native::guarded_real_pair_starts_and_records_actual_native_completion',
        'embedding_pool_process::shared::tests::native::abrupt_service_exit_recovers_real_guarded_helper_before_predecessor_proof',
        'embedding_pool_process::shared::tests::native::actual_guardian_death_contains_helper_and_descendant_while_pool_input_is_open',
    ]
    normal_tests = [
        'embedding_pool_process::shared::tests::shared_recovery_requires_positive_coverage_and_exact_admission',
        'embedding_pool_process::shared::tests::guarded_mode_never_falls_back_for_unqualified_or_unknown_catalog_contracts',
        'embedding_pool_process::shared::tests::admitted_unbound_startup_retains_its_actual_pin_until_every_native_copy_closes',
    ]
    listed, content = step('cli-test-catalog', [binary['airk_cli'], '--list'])
    assert listed == 0 and all(content.splitlines().count(name + ': test') == 1 for name in tests + normal_tests), 'native lifecycle fixture catalog changed'
    lease_listed, lease_content = step('store-pin-catalog', [binary['airk_store'], '--list'])
    lease_count = sum(line.startswith('service::shared_helper_lease::tests::') and line.endswith(': test') for line in lease_content.splitlines())
    assert lease_listed == 0 and lease_count == 2, 'native Store pin catalog changed'
    step('store-native-pin', [binary['airk_store'], 'service::shared_helper_lease::tests::', '--test-threads=1'], 2, 180)
    for name in normal_tests:
        step(name.rsplit('::', 1)[-1], [binary['airk_cli'], '--exact', name, '--test-threads=1', '--nocapture'], 1, 180)
    runtime = destination / 'unsealed-component-pair'
    runtime.mkdir(mode=0o700)
    # Preserve the canonical feature boundary: helper first, copy its exact
    # image, then build the lightweight main in a separate invocation.
    helper_code, helper_text = step('helper-build', ['cargo', 'build', '--locked', '--offline', '-p', 'airk-cli', '--bin', 'airk-embed', '--no-default-features', '--features', 'embedding-worker', '--message-format=json'])
    if helper_code:
        return
    helper = [row for row in artifacts(helper_text) if row.get('target', {}).get('name') == 'airk-embed' and row.get('executable')]
    assert len(helper) == 1 and helper[0].get('fresh') is False
    shutil.copy2(helper[0]['executable'], runtime / 'airk-embed')
    dependency_code, dependency_text = step('lightweight-dependencies', ['cargo', 'tree', '--locked', '--offline', '-p', 'airk-cli', '--no-default-features', '--edges', 'normal', '--prefix', 'none', '--format', '{p}'])
    assert dependency_code == 0
    dependencies = {line.split()[0] for line in dependency_text.splitlines() if line.strip()}
    forbidden = {name for name in dependencies if name in {'tokenizers', 'ort', 'ort-sys'} or name.startswith('tract-')}
    assert not forbidden, 'inference dependency in lightweight main: '+repr(forbidden)
    main_code, main_text = step('main-build', ['cargo', 'build', '--locked', '--offline', '-p', 'airk-cli', '--bin', 'airk', '--no-default-features', '--message-format=json'])
    if main_code:
        return
    main = [row for row in artifacts(main_text) if row.get('target', {}).get('name') == 'airk' and row.get('executable')]
    assert len(main) == 1 and main[0].get('fresh') is False
    shutil.copy2(main[0]['executable'], runtime / 'airk')
    version = tomllib.loads((source / 'Cargo.toml').read_text())['workspace']['package']['version']
    for filename in ['airk', 'airk-embed']:
        with (runtime / filename).open('rb') as image:
            header = image.read(16)
        assert header[:4] == b'\xcf\xfa\xed\xfe' and int.from_bytes(header[4:8], 'little') == 0x0100000c and int.from_bytes(header[12:16], 'little') == 2, 'wrong native component image'
        version_code, version_text = step(filename+'-version', [str(runtime / filename), '--version'], timeout=30)
        assert version_code == 0 and version_text.strip() == filename+' '+version
    pair = {'schema': 1, 'version': version, 'os': 'macos', 'arch': 'aarch64', 'airk_sha256': hashlib.file_digest((runtime / 'airk').open('rb'), 'sha256').hexdigest(), 'embed_sha256': hashlib.file_digest((runtime / 'airk-embed').open('rb'), 'sha256').hexdigest()}
    (runtime / 'airk-runtime.json').write_text(json.dumps(pair)+'\n')
    environment['AIRK_GUARDED_PAIR_MAIN'] = str(runtime / 'airk')
    for name in tests:
        step(name.rsplit('::', 1)[-1], [binary['airk_cli'], '--exact', name, '--ignored', '--test-threads=1', '--nocapture'], 1, 180)
    assert hashlib.file_digest((runtime / 'airk').open('rb'), 'sha256').hexdigest() == pair['airk_sha256'] and hashlib.file_digest((runtime / 'airk-embed').open('rb'), 'sha256').hexdigest() == pair['embed_sha256']
    (destination / 'pair-evidence.json').write_text(json.dumps({'source': manifest['native_source'], 'profile': 'debug', 'scope': 'Unsealed standalone component pair only; not installed, approved or signed release payload', 'runtime_pair': pair, 'pair_receipt_sha256': hashlib.file_digest((runtime / 'airk-runtime.json').open('rb'), 'sha256').hexdigest(), 'test_binary_sha256': {name: hashlib.file_digest(pathlib.Path(value).open('rb'), 'sha256').hexdigest() for name, value in binary.items()}, 'git_metadata_created': False, 'explicit_signing_invoked': False, 'lightweight_dependency_check_passed': True}, indent=2)+'\n')
failure = None
try:
    execute()
except Exception as error:
    failure = repr(error)
after = source_check()
required = {'cli-all-targets-check', 'test-compile', 'store-native-pin', 'shared_recovery_requires_positive_coverage_and_exact_admission', 'guarded_mode_never_falls_back_for_unqualified_or_unknown_catalog_contracts', 'admitted_unbound_startup_retains_its_actual_pin_until_every_native_copy_closes', 'helper-build', 'lightweight-dependencies', 'main-build', 'airk-version', 'airk-embed-version', 'guarded_real_pair_starts_and_records_actual_native_completion', 'abrupt_service_exit_recovers_real_guarded_helper_before_predecessor_proof', 'actual_guardian_death_contains_helper_and_descendant_while_pool_input_is_open'}
if delta_only:
    required = {'cli-all-targets-check', 'test-compile', 'shared_recovery_requires_positive_coverage_and_exact_admission'}
diagnostics = []
for row in steps:
    for line in pathlib.Path(row['log']).read_text(errors='replace').splitlines():
        try:
            diagnostic = json.loads(line)
        except ValueError:
            continue
        if diagnostic.get('reason') == 'compiler-message' and diagnostic.get('message', {}).get('level') in {'warning', 'error'}:
            diagnostics.append({'step': row['name'], 'message': diagnostic['message']})
passed = failure is None and required <= {row['name'] for row in steps} and all(row['exit_code'] == 0 and row['sufficient_test_count'] for row in steps) and (not delta_only or not diagnostics)
result = {'schema': 'airk.private-guarded-provider-qualification/1', 'status': 'passed' if passed else 'failed', 'scope': 'Unix warning corrections and one previously failing native runtime-child fixture only' if delta_only else 'Focused actual Unix guarded helper startup, guardian loss, native completion, predecessor recovery/GC and Store pins', 'platform': platform.platform(), 'native_source': manifest['native_source'], 'source_before': before, 'source_after': after, 'steps': steps, 'compiler_diagnostics': diagnostics, 'warning_free_required': delta_only, 'harness_error': failure, 'not_run': ['full P6 checkpoint', 'full workspace test execution', 'real model inference', 'held engine-work/sibling completion', 'installed editor', 'Developer ID signing/notarization', 'Linux qualification'] + (['native pair rebuild', 'previously passing guarded lifecycle fixtures', 'previously passing native pin and shared unit fixtures'] if delta_only else []), 'retained_control_or_user_install_changed': False}
(destination / 'qualification.json').write_text(json.dumps(result, indent=2)+'\n')
print(json.dumps(result, indent=2), flush=True)
