# SPDX-License-Identifier: Apache-2.0
"""Run full source-bound Neo qualification; preserve every stage and clean owned Cargo output."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import resource
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import xml.etree.ElementTree as ET

run = Path(__file__).resolve().parent
assert run.parent == Path.home() / 'airk-builds' and run.name.startswith('upgrade-1.8.9-full-')
assert platform.system() == 'Darwin' and platform.machine() == 'arm64'
source, target = run / 'source', run / 'target'
tools = Path('/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools')
inputs = Path('/Users/tylerjensen/airk-builds/cargo-clean-69d305e3933f/inputs')
snapshot = json.loads((run / 'snapshot.json').read_text())
steps = []
environment = dict(os.environ, PATH=str(tools / 'node-v24.15.0-darwin-arm64/bin') + ':' + str(tools) + ':' + str(Path.home() / '.cargo/bin') + ':/opt/homebrew/bin:' + os.environ.get('PATH', ''), CARGO_BUILD_JOBS='1', CARGO_NET_OFFLINE='true', CARGO_INCREMENTAL='0', MACOSX_DEPLOYMENT_TARGET='13.3', PYTHONUTF8='1', CARGO_TARGET_DIR=str(target), AIRK_BUNDLED_MODEL_DIR=str(inputs / 'local-vector-model'), AIRK_BUNDLED_RUNTIME_DIR=str(inputs / 'local-onnx-runtime'), AIRK_TEST_MODEL_DIR=str(inputs / 'local-vector-model/minilm-l6-v2-s8-v1'))
# Match the previously qualified Neo harness: protected fixture reads start at
# their actual canonical temporary root rather than the /var symlink spelling.
environment['TMPDIR'] = str(Path(tempfile.gettempdir()).resolve(strict=True))
for name in ('AIRK_TEST_BINARY', 'AIRK_NATIVE_TEST_BINARY', 'AIRK_CATALOG_CANDIDATE_BINARY', 'AIRK_LEGACY_TEST_BINARY', 'AIRK_LEGACY_TEST_MODEL_DIR', 'AIRK_NATIVE_DIAGNOSTIC_DIR', 'AIRK_P10_RESOURCE_OUTPUT_DIR', 'AIRK_P11B_RESOURCE_OUTPUT_DIR'):
    environment.pop(name, None)
soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
assert hard == resource.RLIM_INFINITY or hard >= 4096
resource.setrlimit(resource.RLIMIT_NOFILE, (4096, hard))

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def save(name, value):
    (run / name).write_text(json.dumps(value, indent=2) + '\n')

def verify():
    for name, expected in snapshot['files'].items():
        assert sha(source / name) == expected, name

def stage(name, command, required=(), cwd=None):
    (run / 'phase.txt').write_text(name)
    started = time.monotonic()
    skipped = any(not any(row['name'] == dependency and row['exit_code'] == 0 for row in steps) for dependency in required)
    with (run / (name + '.log')).open('x') as output:
        if skipped:
            output.write('Prerequisite failed; dependent command was not launched.\n')
            code = 125
        else:
            code = subprocess.run(command, cwd=cwd or source, env=environment, stdout=output, stderr=subprocess.STDOUT).returncode
    row = {'name': name, 'command': command, 'cwd': str(cwd or source), 'seconds': round(time.monotonic() - started, 3), 'exit_code': code, 'not_launched': skipped}
    steps.append(row)
    save('steps.json', steps)
    print(json.dumps(row), flush=True)
    return code

error = None
try:
    assert not source.exists() and not target.exists()
    assert sha(run / 'source.tar.gz') == snapshot['archive_sha256']
    source.mkdir(mode=0o700)
    with tarfile.open(run / 'source.tar.gz') as archive:
        members = archive.getmembers()
        assert len(members) == len(snapshot['files']) and {member.name for member in members} == set(snapshot['files'])
        for member in members:
            assert member.isfile() and not Path(member.name).is_absolute() and (source / member.name).resolve().is_relative_to(source)
        archive.extractall(source, filter='data')
    verify()
    subprocess.run(['git', 'init', '--quiet'], cwd=source, check=True)
    subprocess.run(['git', 'add', '--all'], cwd=source, check=True)
    # Retain the real branch entry commit for fixtures inspecting HEAD; source hashes disclose uncommitted changes.
    commit, tree = (run / 'head.commit').read_bytes(), (run / 'head.tree').read_bytes()
    original_tree = commit.splitlines()[0].decode().removeprefix('tree ')
    assert subprocess.check_output(['git', 'hash-object', '-t', 'tree', '-w', '--stdin'], input=tree, cwd=source).decode().strip() == original_tree
    assert subprocess.check_output(['git', 'hash-object', '-t', 'commit', '-w', '--stdin'], input=commit, cwd=source).decode().strip() == snapshot['entry_commit']
    subprocess.run(['git', 'update-ref', 'HEAD', snapshot['entry_commit']], cwd=source, check=True)
    private = run / 'private'
    private.mkdir(mode=0o700)
    environment['AIRK_PRIVATE_CENSUS_BASE'] = str(private)
    environment['AIRK_NATIVE_DIAGNOSTIC_DIR'] = str(run / 'native-diagnostics')
    (private / 'empty.ignore').write_text('')
    (private / 'gitconfig').write_text('[core]\nexcludesFile = "' + str(private / 'empty.ignore') + '"\n')
    environment['GIT_CONFIG_GLOBAL'] = str(private / 'gitconfig')
    specification = importlib.util.spec_from_file_location('runtime_builder', source / 'scripts/build-service-runtime.py')
    builder = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(builder)
    assert builder.source_identity() == snapshot['native_source']
    stage('tools', ['sh', '-c', 'rustc --version && cargo nextest --version && node --version && npm --version && python3 --version'])
    stage('format', ['cargo', 'fmt', '--all', '--', '--check'])
    stage('clippy', ['cargo', 'clippy', '--workspace', '--all-targets', '--all-features', '--locked', '--offline', '--timings', '--', '-D', 'warnings'], ['format'])
    stage('lightweight', ['cargo', 'check', '-p', 'airk-cli', '--no-default-features', '--all-targets', '--locked', '--offline', '--timings'], ['format'])
    stage('runtime', ['python3', 'scripts/build-service-runtime.py', '--destination', str(run / 'runtime-debug'), '--profile', 'debug'], ['format'])
    binary = run / 'runtime-debug/AIRK.app/Contents/MacOS/airk'
    if binary.is_file():
        environment['AIRK_TEST_BINARY'] = str(binary)
        environment['AIRK_NATIVE_TEST_BINARY'] = str(binary)
    stage('workspace-build', ['cargo', 'build', '--workspace', '--locked', '--offline', '--timings'], ['format'])
    stage('native-tests', ['cargo', 'nextest', 'run', '--workspace', '--all-features', '--locked', '--offline', '--profile', 'ci', '--test-threads', '2', '--ignore-default-filter', '--retries', '0', '--no-fail-fast', '--timings'], ['format'])
    junit = source / 'target/nextest/ci/junit.xml'
    if junit.is_file():
        shutil.copyfile(junit, run / 'native-tests.junit.xml')
        cases = list(ET.parse(junit).iter('testcase'))
        campaigns = {}
        for campaign in ('compact_measurements_compare_small_reads_from_large_catalog', 'two_hundred_current_edits_reach_object_and_database_plateau', 'measure_small_and_larger_sparse_catalog_edits', 'manual_backlog_larger_than_one_page_advances_without_unprotected_deletion'):
            selected = [case for case in cases if case.get('name', '').rsplit('::', 1)[-1] == campaign]
            campaigns[campaign] = {'matched_cases': len(selected), 'passed': len(selected) == 1 and all(selected[0].find(tag) is None for tag in ('failure', 'error', 'skipped')), 'seconds': selected[0].get('time') if len(selected) == 1 else None}
        save('native-counts.json', {'total': len(cases), **{tag: sum(case.find(tag) is not None for case in cases) for tag in ('failure', 'error', 'skipped')}, 'routine_campaigns': campaigns, 'four_routine_excluded_campaigns_restored': all(row['matched_cases'] == 1 for row in campaigns.values())})
        if not all(row['matched_cases'] == 1 for row in campaigns.values()):
            steps[-1]['exit_code'] = steps[-1]['exit_code'] or 1
            steps[-1]['validation_error'] = 'Full native selection omitted or duplicated a required routine campaign'
            save('steps.json', steps)
    else:
        steps[-1]['exit_code'] = steps[-1]['exit_code'] or 1
        steps[-1]['validation_error'] = 'Native JUnit report missing'
        save('steps.json', steps)
    stage('doctests', ['cargo', 'test', '--doc', '--workspace', '--all-features', '--locked', '--offline', '--timings', '--', '--test-threads=1'], ['format'])
    stage('service-clients', ['python3', 'scripts/check-service-clients.py', '--runtime-dir', str(run / 'runtime-debug'), '--files', '16', '--model'], ['runtime'])
    stage('npm-install', ['npm', 'ci', '--ignore-scripts', '--no-audit', '--no-fund', '--offline', '--prefix', 'packages/vscode'])
    stage('extension-compile', ['npm', 'run', 'compile', '--prefix', 'packages/vscode'], ['npm-install'])
    stage('extension-tests', ['sh', '-c', 'node --test --test-concurrency=1 out/test/*.test.js'], ['extension-compile', 'runtime'], cwd=source / 'packages/vscode')
    stage('python-tests', ['python3', '-m', 'unittest', 'discover', '-s', 'scripts/tests', '-v'])
    stage('notices', ['python3', 'scripts/license-notices.py', '--output', str(run / 'notices'), '--release'])
    stage('repository', ['pwsh', '-NoProfile', '-File', 'scripts/check-repo.ps1'])
    stage('whitespace', ['git', 'diff', '--check'])
    verify()
    assert builder.source_identity() == snapshot['native_source']
except BaseException as failure:
    error = repr(failure)
    print(error, flush=True)
finally:
    save('qualification.json', {'schema': 'airk.upgrade-1.8.9.neo-full/1', 'entry_commit': snapshot['entry_commit'], 'native_source': snapshot['native_source'], 'archive_sha256': snapshot['archive_sha256'], 'platform': platform.platform(), 'rlimit_nofile_initial': [soft, hard], 'rlimit_nofile_run': [4096, hard], 'environment': {name: environment.get(name) for name in ['TMPDIR', 'CARGO_TARGET_DIR', 'CARGO_BUILD_JOBS', 'CARGO_INCREMENTAL', 'AIRK_TEST_BINARY', 'AIRK_TEST_MODEL_DIR', 'MACOSX_DEPLOYMENT_TARGET']}, 'steps': steps, 'error': error, 'status': 'passed' if error is None and steps and all(step['exit_code'] == 0 for step in steps) else 'failed', 'not_run': ['Dependency advisory audit: cargo-audit/cargo-deny/cargo-vet absent on Neo; no dependency changes; Windows owner retains canonical audit scope', 'Hosted CI, Intel macOS, publication and installed normal-user profile qualification']})
    (run / 'phase.txt').write_text('tests-finished')
sys.exit(0 if error is None and steps and all(step['exit_code'] == 0 for step in steps) else 1)
