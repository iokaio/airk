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

def stage(name, command, required=(), cwd=None, stdout_file=None):
    (run / 'phase.txt').write_text(name)
    started = time.monotonic()
    skipped = any(not any(row['name'] == dependency and row['exit_code'] == 0 for row in steps) for dependency in required)
    with (run / (name + '.log')).open('x') as output:
        if skipped:
            output.write('Prerequisite failed; dependent command was not launched.\n')
            code = 125
        elif stdout_file is not None:
            with stdout_file.open('x') as machine:
                code = subprocess.run(command, cwd=cwd or source, env=environment, stdout=machine, stderr=output).returncode
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
    baseline = json.loads((run / 'baseline.json').read_text())
    original = baseline['original_compiled_source']
    assert snapshot['native_source'] == {'sha256':'f200722c2bf21be2c443384f7daf415667b4fafe5c52d5245dd9b69deba2f50a','files':978}
    allowed = set(baseline['allowed_future_corrections_only'])
    assert allowed == {'crates/airk-cli/tests/mcp.rs','crates/airk-cli/tests/service_runtime.rs','crates/airk-cli/tests/common/read_retry.rs','crates/airk-cli/tests/native_read_retry.rs'}
    for name, expected in baseline['original_native_inputs'].items():
        if name not in allowed:
            assert snapshot['files'][name] == expected, 'Non-test native input changed: ' + name
    expected_document = 'd0be082a29064fdb932dc98b862ceccadcf588fcb59345403d98a14122f05ea8'
    assert snapshot['files']['docs/cli-reference.md'] == expected_document
    runtime = Path('/Users/tylerjensen/airk-builds/upgrade-1.8.9-full-r5/runtime-debug')
    build = json.loads((runtime / 'airk-build.json').read_text())
    assert build['source'] == original, 'Original runtime compiled-source identity differs'
    install_bytes = (runtime / 'airk-install.json').read_bytes()
    install = json.loads(install_bytes)
    assert install['version'] == '1.8.9' and len(install['files']) == 678
    def verify_runtime():
        actual = {path.relative_to(runtime).as_posix():sha(path) for path in runtime.rglob('*') if path.is_file()}
        expected = {**install['files'],'airk-install.json':hashlib.sha256(install_bytes).hexdigest()}
        assert actual == expected, 'Complete original runtime payload seal differs'
    verify_runtime()
    save('runtime-reuse.json', {'qualification':'original compiled acc3 runtime reuse, not a fresh native build','original_runtime':str(runtime),'original_compiled_source':original,'current_test_source':snapshot['native_source'],'unchanged_native_inputs':len(baseline['original_native_inputs'])-len(allowed),'allowed_external_tests':sorted(allowed),'generated_document_sha256':expected_document,'complete_artifact_seal_files':len(install['files']),'install_manifest_sha256':hashlib.sha256(install_bytes).hexdigest(),'build_receipt_sha256':sha(runtime / 'airk-build.json')})
    binary = runtime / 'AIRK.app/Contents/MacOS/airk'
    environment['AIRK_TEST_BINARY'] = environment['AIRK_NATIVE_TEST_BINARY'] = str(binary)
    stage('tools', ['sh', '-c', 'rustc --version && cargo nextest --version && node --version && python3 --version'])
    stage('npm-install', ['npm','ci','--ignore-scripts','--no-audit','--no-fund','--offline','--prefix','packages/vscode'])
    stage('extension-compile', ['npm','run','compile','--prefix','packages/vscode'], ['npm-install'])
    stage('editor-helper-tests', ['node','--test','scripts/tests/editor-ui.test.cjs'])
    stage('extension-tests', ['sh','-c','node --test --test-concurrency=1 out/test/*.test.js'], ['extension-compile'], cwd=source / 'packages/vscode')
    stage('python-tests', ['python3','-m','unittest','discover','-s','scripts/tests','-v'])
    stage('notices', ['python3','scripts/license-notices.py','--output',str(run/'notices'),'--release'])
    stage('repository', ['pwsh','-NoProfile','-File','scripts/check-repo.ps1'])
    stage('whitespace', ['git','diff','--check'])
    verify_runtime()
    verify()
    assert builder.source_identity() == snapshot['native_source']
except BaseException as failure:
    error = repr(failure)
    print(error, flush=True)
finally:
    save('qualification.json', {'schema': 'airk.upgrade-1.8.9.neo-final-ts-after-native-correctives/1', 'entry_commit': snapshot['entry_commit'], 'native_source': snapshot['native_source'], 'archive_sha256': snapshot['archive_sha256'], 'platform': platform.platform(), 'rlimit_nofile_initial': [soft, hard], 'rlimit_nofile_run': [4096, hard], 'environment': {name: environment.get(name) for name in ['TMPDIR', 'CARGO_TARGET_DIR', 'CARGO_BUILD_JOBS', 'CARGO_INCREMENTAL', 'AIRK_TEST_BINARY', 'AIRK_TEST_MODEL_DIR', 'MACOSX_DEPLOYMENT_TARGET']}, 'steps': steps, 'error': error, 'status': 'passed' if error is None and steps and all(step['exit_code'] == 0 for step in steps) else 'failed', 'not_run': ['Unchanged full native workspace, four long campaigns, doctests and model clients retain original r5 observations; corrected external tests retain21/21 dedicated corrective results. No repeated native compilation or fresh runtime build claimed', 'Hosted CI, Intel macOS, publication and installed normal-user profile qualification']})
    (run / 'phase.txt').write_text('tests-finished')
sys.exit(0 if error is None and steps and all(step['exit_code'] == 0 for step in steps) else 1)
