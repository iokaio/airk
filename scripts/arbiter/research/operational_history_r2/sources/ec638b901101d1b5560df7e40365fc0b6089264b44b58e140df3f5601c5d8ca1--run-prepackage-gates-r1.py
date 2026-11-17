# SPDX-License-Identifier: Apache-2.0
import hashlib, importlib.util, json, os, subprocess, sys, time
from pathlib import Path
base = Path(__file__).resolve().parent
root = base.parents[1]
out = base / 'windows-r1'
runtime = out / 'release-runtime'
prior = json.loads((out / 'final-rust-gates-r1.json').read_text(encoding='utf-8'))
assert len(prior['results']) == 4 and all(row['exit_code'] == 0 for row in prior['results'])
spec = importlib.util.spec_from_file_location('native_package', root / 'scripts/package-native.py')
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)
source = json.loads((base / 'source-freeze-r3.json').read_text(encoding='utf-8'))['native_source']
sealed, executables = native.validate_runtime(runtime, '1.8.8', 'x86_64-pc-windows-msvc', source)
build_sha = hashlib.sha256((runtime / 'airk-build.json').read_bytes()).hexdigest()
with (out / 'prepackage-runtime-seal-r1.json').open('x', encoding='utf-8') as stream:
    json.dump({'native_source': source, 'airk_build_sha256': build_sha, 'files': sealed, 'executables': executables}, stream, indent=2)
env = os.environ.copy()
env['AIRK_PRIVATE_CENSUS_BASE'] = str(root / 'artifacts/v190/p10-private-fixtures-619c272e8a644988b26bbe4334fc8c02')
env['AIRK_DEV_CARGO_INCREMENTAL'] = '1'
steps = [
    ('full-node-r1.log', ['python', str(root / 'artifacts/v190/pr60-root-node-owner-r1.py'), '--runtime', str(runtime), '--output', str(root / 'artifacts/v190/branch-review-full-node-r1.json')]),
    ('clients16model-r1.log', ['python', 'scripts/check-service-clients.py', '--runtime-dir', str(runtime), '--files', '16', '--model', '--report', str(out / 'clients16model-r1.json')]),
    ('owned-install-r1.log', ['python', 'scripts/check-service-install.py', '--runtime', str(runtime)]),
    ('python-contracts-r2.log', ['python', '-m', 'unittest', 'discover', '-s', 'scripts/tests', '-v']),
    ('dependency-audit-r1.log', ['python', 'scripts/dependency-audit.py']),
    ('license-notices-r1.log', ['python', 'scripts/license-notices.py', '--output', str(out / 'notices'), '--release']),
]
receipt = out / 'prepackage-gates-r1.json'
assert not receipt.exists()
results = []
for label, command in steps:
    print(json.dumps({'stage': label, 'command': command}), flush=True)
    start = time.monotonic()
    code = subprocess.run([sys.executable, '-X', 'utf8', str(base / 'command-r1.py'), label, *command], cwd=root, env=env).returncode
    assert native.inventory(runtime) == sealed, 'Runtime changed during ' + label
    assert hashlib.sha256((runtime / 'airk-build.json').read_bytes()).hexdigest() == build_sha
    results.append({'stage': label, 'exit_code': code, 'seconds': time.monotonic() - start})
    receipt.write_text(json.dumps({'results': results}, indent=2), encoding='utf-8')
    if code: sys.exit(code)
