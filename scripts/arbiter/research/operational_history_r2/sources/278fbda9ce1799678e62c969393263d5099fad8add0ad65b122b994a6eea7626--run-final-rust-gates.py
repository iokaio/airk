# SPDX-License-Identifier: Apache-2.0
import hashlib, json, os, shutil, subprocess, sys, time
from pathlib import Path

root = Path(__file__).resolve().parents[2]
base = Path(__file__).resolve().parent
env = os.environ.copy()
env['AIRK_PRIVATE_CENSUS_BASE'] = str(root / 'artifacts/v190/p10-private-fixtures-619c272e8a644988b26bbe4334fc8c02')
full = json.loads((base / 'windows-r1/full-native-r2.summary.json').read_text(encoding='utf-8'))
assert full['exit_code'] == 0 and full['passed'] == full['selected'] and not full['failed']
steps = [
    ('doctests-r1.log', ['cargo', 'test', '--doc', '--workspace', '--all-features', '--locked', '--offline', '--timings', '--', '--test-threads=1'], '1'),
    ('lightweight-main-r1.log', ['cargo', 'check', '-p', 'airk-cli', '--no-default-features', '--all-targets', '--locked', '--offline', '--timings'], '1'),
    ('workspace-build-r1.log', ['cargo', 'build', '--workspace', '--locked', '--offline', '--timings'], '1'),
    ('release-runtime-r1.log', ['python', 'scripts/build-service-runtime.py', '--destination', str(base / 'windows-r1/release-runtime'), '--profile', 'release'], '0'),
]
receipt = base / 'windows-r1/final-rust-gates-r1.json'
assert not receipt.exists()
results = []
for label, command, incremental in steps:
    env['AIRK_DEV_CARGO_INCREMENTAL'] = incremental
    print(json.dumps({'stage': label, 'command': command, 'incremental': incremental}), flush=True)
    start = time.monotonic()
    timing_root = root / 'artifacts/v190/p10-target/cargo-timings'
    before = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in timing_root.glob('*.html')}
    code = subprocess.run([sys.executable, '-X', 'utf8', str(base / 'command-r1.py'), label, *command], cwd=root, env=env).returncode
    timing_dir = base / 'windows-r1/timings' / Path(label).stem
    timing_dir.mkdir(parents=True, exist_ok=False)
    timings = []
    for path in timing_root.glob('*.html'):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        if before.get(path.name) != digest:
            shutil.copyfile(path, timing_dir / path.name)
            timings.append({'path': str(timing_dir / path.name), 'sha256': digest})
    results.append({'stage': label, 'exit_code': code, 'seconds': time.monotonic() - start, 'timings': timings})
    receipt.write_text(json.dumps({'results': results}, indent=2), encoding='utf-8')
    if code:
        sys.exit(code)
