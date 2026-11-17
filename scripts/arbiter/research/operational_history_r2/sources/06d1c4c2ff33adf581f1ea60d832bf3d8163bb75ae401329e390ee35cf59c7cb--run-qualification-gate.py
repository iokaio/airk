# SPDX-License-Identifier: Apache-2.0
import argparse, hashlib, importlib.util, json, os, shutil, subprocess, time
from pathlib import Path

base = Path(__file__).resolve().parent
root = base.parents[2]
os.chdir(root)
parser = argparse.ArgumentParser()
parser.add_argument('--runtime', type=Path)
parser.add_argument('name')
parser.add_argument('command', nargs=argparse.REMAINDER)
args = parser.parse_args()
command = args.command
assert command and '/' not in args.name and '\\' not in args.name
if command[0] == 'cargo':
    command[0] = 'C:/Users/tdotn/.cargo/bin/cargo.exe'
report = json.loads((base / 'static-environment-supplement.json').read_text())['environment']
assert set(report) == {'LIBONNXRUNTIME_NO_PKG_CONFIG', 'ORT_SKIP_DOWNLOAD', 'ORT_PREFER_DYNAMIC_LINK', 'ORT_LIB_LOCATION', 'ORT_LIB_PROFILE'}
env = {**os.environ, **report}
target = root / 'artifacts/v190/p10-target'
private = json.loads((base / 'user-private-fixture-base.json').read_text())['path']
env.update(PATH=str(root/'artifacts/tools/nextest') + os.pathsep + 'C:/Users/tdotn/.cargo/bin' + os.pathsep + env['PATH'],
           CARGO_TARGET_DIR=str(target), CARGO_INCREMENTAL='1', TEMP=private, TMP=private,
           AIRK_PRIVATE_CENSUS_BASE=private, AIRK_TEST_MODEL_DIR=str(root/'target/local-vector-model/minilm-l6-v2-u8-v1'),
           GIT_CONFIG_COUNT='1', GIT_CONFIG_KEY_0='safe.directory', GIT_CONFIG_VALUE_0=root.as_posix())
runtime_proof = None
if args.runtime:
    runtime = args.runtime.resolve(strict=True)
    specification = importlib.util.spec_from_file_location('gate_runtime_builder', root/'scripts/build-service-runtime.py')
    builder = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(builder)
    compiled = json.loads((runtime/'airk-build.json').read_text())['source']
    assert builder.source_identity() == compiled, 'Native reuse source mismatch'
    manifest = json.loads((runtime/'airk-install.json').read_text())
    for name, expected in manifest['files'].items():
        path = (runtime/name).resolve(strict=True)
        assert path.is_relative_to(runtime) and hashlib.sha256(path.read_bytes()).hexdigest() == expected, name
    assert {p.relative_to(runtime).as_posix() for p in runtime.rglob('*') if p.is_file()} == set(manifest['files'])|{'airk-install.json'}
    env.update(AIRK_TEST_BINARY=str(runtime/'airk.exe'), AIRK_NATIVE_TEST_BINARY=str(runtime/'airk.exe'), AIRK_TEST_F09_QUALIFICATION='1')
    runtime_proof = dict(compiled_source=compiled, sealed_files=len(manifest['files']),
                         runtime_manifest_sha256=hashlib.sha256((runtime/'airk-install.json').read_bytes()).hexdigest(),
                         reuse='Original sealed runtime reused; current extension/test identities recorded separately',
                         extension_sources={p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in (root/'packages/vscode/src').rglob('*') if p.is_file()})
def source():
    files = [root/'Cargo.toml', root/'Cargo.lock']
    files.extend(p for p in (root/'crates').rglob('*') if p.is_file())
    return {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(files)}
before = source()
shared_test_paths = [root/'packages/vscode/src/test/fixtures/httpAuthorityDigest.json']
shared_test_before = {p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in shared_test_paths if p.is_file()}
script_before = {p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in (root/'scripts').rglob('*') if p.is_file() and p.suffix in ('.py','.cjs','.ps1','.json')}
start = time.monotonic()
start_unix = time.time()
with (base / (args.name+'.log')).open('x', encoding='utf-8') as log:
    result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT)
receipt = dict(arguments=command, exit_code=result.returncode, elapsed_seconds=time.monotonic()-start,
               started_unix=start_unix, target_directory=str(target), incremental='on',
               native_inputs_unchanged=before == source(), source_files=before,
               execution='contributor identity; existing protected fixtures; one Cargo owner')
receipt['script_sources'] = script_before
receipt['shared_test_sources'] = shared_test_before
receipt['shared_test_inputs_unchanged'] = shared_test_before == {p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in shared_test_paths if p.is_file()}
receipt['script_inputs_unchanged'] = script_before == {p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in (root/'scripts').rglob('*') if p.is_file() and p.suffix in ('.py','.cjs','.ps1','.json')}
if runtime_proof:
    receipt['runtime_reuse'] = runtime_proof
    receipt['extension_inputs_unchanged'] = runtime_proof['extension_sources'] == {p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in (root/'packages/vscode/src').rglob('*') if p.is_file()}
timings = target/'cargo-timings/cargo-timing.html'
if any('cargo' in part or 'dev-check.py' in part or 'build-service-runtime.py' in part for part in command) and timings.exists() and timings.stat().st_mtime >= start_unix:
    retained = base/(args.name+'-cargo-timing.html')
    shutil.copy2(timings, retained)
    receipt['cargo_timings'] = retained.name
if 'nextest' in command and 'run' in command:
    for junit in (target/'nextest/ci/junit.xml', root/'target/nextest/ci/junit.xml'):
        if junit.is_file() and junit.stat().st_mtime >= start_unix:
            retained = base/(args.name+'-junit.xml')
            shutil.copy2(junit, retained)
            receipt['junit'] = retained.name
            break
with (base / (args.name+'.json')).open('x') as out:
    json.dump(receipt, out, indent=2)
print(json.dumps({k:v for k,v in receipt.items() if k not in ('source_files', 'runtime_reuse', 'script_sources')}), flush=True)
raise SystemExit(result.returncode)
