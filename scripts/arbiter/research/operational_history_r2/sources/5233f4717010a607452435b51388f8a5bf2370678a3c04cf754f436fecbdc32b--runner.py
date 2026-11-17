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
for name in ('AIRK_TEST_BINARY', 'AIRK_CATALOG_CANDIDATE_BINARY', 'AIRK_P10_RESOURCE_OUTPUT_DIR', 'AIRK_P11B_RESOURCE_OUTPUT_DIR'):
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

def stage(name, command, required=()):
    (run / 'phase.txt').write_text(name)
    started = time.monotonic()
    skipped = any(not any(row['name'] == dependency and row['exit_code'] == 0 for row in steps) for dependency in required)
    with (run / (name + '.log')).open('x') as output:
        if skipped:
            output.write('Prerequisite failed; dependent command was not launched.\n')
            code = 125
        else:
            code = subprocess.run(command, cwd=source, env=environment, stdout=output, stderr=subprocess.STDOUT).returncode
    row = {'name': name, 'command': command, 'seconds': round(time.monotonic() - started, 3), 'exit_code': code, 'not_launched': skipped}
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
    (private / 'empty.ignore').write_text('')
    (private / 'gitconfig').write_text('[core]\nexcludesFile = "' + str(private / 'empty.ignore') + '"\n')
    environment['GIT_CONFIG_GLOBAL'] = str(private / 'gitconfig')
    specification = importlib.util.spec_from_file_location('runtime_builder', source / 'scripts/build-service-runtime.py')
    builder = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(builder)
    assert builder.source_identity() == snapshot['native_source']
    stage('tools', ['sh', '-c', 'node --version && npm --version && python3 --version'])
    runtime = Path('/Users/tylerjensen/airk-builds/upgrade-1.8.9-full-r2/runtime-debug')
    built = json.loads((runtime / 'airk-build.json').read_text())
    assert built['source'] == snapshot['native_source'], 'Native inputs changed; original compiled runtime cannot be reused'
    install = json.loads((runtime / 'airk-install.json').read_text())
    assert install['version'] == '1.8.9' and len(install['files']) < 2048
    def verify_runtime():
        for name, expected in install['files'].items():
            assert sha(runtime / name) == expected, name
    verify_runtime()
    save('runtime-reuse.json', {'qualification': 'reuse of original compiled runtime, not a fresh build', 'original_runtime': str(runtime), 'original_build_source': built['source'], 'current_native_source': snapshot['native_source'], 'complete_seal_verified': True, 'files': len(install['files']), 'install_manifest_sha256': sha(runtime / 'airk-install.json'), 'build_receipt_sha256': sha(runtime / 'airk-build.json')})
    environment['AIRK_TEST_BINARY'] = str(runtime / 'AIRK.app/Contents/MacOS/airk')
    stage('npm-install', ['npm', 'ci', '--ignore-scripts', '--no-audit', '--no-fund', '--offline', '--prefix', 'packages/vscode'])
    stage('extension-tests', ['npm', 'test', '--prefix', 'packages/vscode'], ['npm-install'])
    stage('repository', ['pwsh', '-NoProfile', '-File', 'scripts/check-repo.ps1'])
    stage('whitespace', ['git', 'diff', '--check'])
    verify_runtime()
    verify()
    assert builder.source_identity() == snapshot['native_source']
except BaseException as failure:
    error = repr(failure)
    print(error, flush=True)
finally:
    save('qualification.json', {'schema': 'airk.upgrade-1.8.9.neo-final-extension/1', 'entry_commit': snapshot['entry_commit'], 'native_source': snapshot['native_source'], 'archive_sha256': snapshot['archive_sha256'], 'platform': platform.platform(), 'rlimit_nofile_initial': [soft, hard], 'rlimit_nofile_run': [4096, hard], 'environment': {name: environment.get(name) for name in ['CARGO_TARGET_DIR', 'CARGO_BUILD_JOBS', 'CARGO_INCREMENTAL', 'AIRK_TEST_BINARY', 'AIRK_TEST_MODEL_DIR', 'MACOSX_DEPLOYMENT_TARGET']}, 'steps': steps, 'error': error, 'status': 'passed' if error is None and steps and all(step['exit_code'] == 0 for step in steps) else 'failed', 'not_run': ['Native suites: full r2 campaign separately qualifies unchanged native inputs; this batch reuses the exact sealed r2 debug runtime', 'Hosted CI, Intel macOS, publication and installed normal-user profile qualification']})
    (run / 'phase.txt').write_text('tests-finished')
sys.exit(0 if error is None and steps and all(step['exit_code'] == 0 for step in steps) else 1)
