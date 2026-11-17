# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime, hashlib, importlib.util, json, os, subprocess, sys, time

root = Path.cwd()
artifacts = root / 'artifacts/optimizations'
location = json.loads((artifacts / 'f06-authority-runtime-location.json').read_text())
task, runtime = Path(location['task']), Path(location['runtime'])
kind, approved = sys.argv[1:]
assert hashlib.sha256((runtime / 'airk-install.json').read_bytes()).hexdigest() == approved
for item in location['scripts']:
    assert hashlib.sha256((task / 'scripts' / item['name']).read_bytes()).hexdigest() == item['sha256']
spec = importlib.util.spec_from_file_location('f06_native_builder', root / 'scripts/build-service-runtime.py')
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
env = os.environ.copy()
env.update(TEMP=str(task / 'fixtures'), TMP=str(task / 'fixtures'))
cwd = root
common = ['--runtime', str(runtime), '--approved-sha256', approved, '--task-root', str(task),
          '--protected-root', str(root), '--protected-root', 'D:/code/Github/bookbuilder']
if kind in ['smoke-terse', 'smoke-full-basic']:
    env.update(AIRK_TEST_BINARY=str(runtime / 'airk.exe'), AIRK_MCP_RESPONSE_MODE='terse' if kind == 'smoke-terse' else 'full')
    cwd = root / 'packages/vscode'
    command = ['node', '--test']
    if kind == 'smoke-full-basic':
        command += ['--test-name-pattern', 'basic status includes version']
    command += ['out/test/nativeSmoke.test.js']
elif kind == 'eof':
    diagnostic = task / 'reports' / 'native-eof'
    diagnostic.mkdir()
    env.update(CARGO_BUILD_JOBS='4', CARGO_NET_OFFLINE='true',
               AIRK_BUNDLED_MODEL_DIR=str(root / 'target/local-vector-model'),
               AIRK_BUNDLED_RUNTIME_DIR=str(root / 'target/local-onnx-runtime'),
               AIRK_F03_RUNTIME=str(runtime), AIRK_F03_APPROVED_SHA256=approved,
               AIRK_NATIVE_EOF_DIAGNOSTIC_DIR=str(diagnostic))
    command = ['C:/Users/tdotn/.cargo/bin/cargo.exe', 'test', '-p', 'airk-cli', '--lib', '--all-features',
               '--locked', '--offline', 'runtime_install::native_connection_tests::native_mcp_',
               '--', '--ignored', '--nocapture', '--test-threads=1']
elif kind == 'healthy':
    command = [sys.executable, str(task / 'scripts/f06-healthy-status-isolated.py'), *common,
               '--report-tag', 'healthy-status-isolated-01']
elif kind == 'scope':
    command = [sys.executable, str(task / 'scripts/check-source-scope-original-identity.py'), *common,
               '--report-tag', 'source-scope-01']
elif kind == 'clients':
    command = [sys.executable, str(task / 'scripts/check-service-clients.py'), '--runtime-dir', str(runtime),
               '--files', '4', '--model', '--report', str(task / 'reports/clients-original.json')]
else:
    raise ValueError('unknown selected gate')
record = {'command': command, 'cwd': str(cwd), 'approved_install_sha256': approved,
          'source_before': builder.source_identity(), 'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'environment': {k: env[k] for k in ['TEMP', 'TMP', 'AIRK_TEST_BINARY', 'AIRK_MCP_RESPONSE_MODE',
                                           'AIRK_F03_RUNTIME', 'AIRK_F03_APPROVED_SHA256', 'AIRK_NATIVE_EOF_DIAGNOSTIC_DIR'] if k in env}}
receipt = artifacts / f'f06-authority-native-{kind}-execution.json'
with receipt.open('x') as stream:
    json.dump(record, stream, indent=2)
started = time.monotonic()
with (artifacts / f'f06-authority-native-{kind}.log').open('x', encoding='utf-8') as output:
    result = subprocess.run(command, cwd=cwd, env=env, stdout=output, stderr=subprocess.STDOUT)
record.update(exit_code=result.returncode, elapsed_seconds=time.monotonic() - started,
              ended_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(), source_after=builder.source_identity())
record['source_unchanged'] = record['source_before'] == record['source_after']
receipt.write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps(record), flush=True)
raise SystemExit(result.returncode or (0 if record['source_unchanged'] else 1))
