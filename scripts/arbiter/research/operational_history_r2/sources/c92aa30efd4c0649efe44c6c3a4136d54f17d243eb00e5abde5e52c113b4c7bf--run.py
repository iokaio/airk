# SPDX-License-Identifier: Apache-2.0
"""Local qualification owner; retain every command, result and Cargo timing."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

sys.stdout.reconfigure(encoding='utf-8')

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent
TARGET = ROOT / 'artifacts/v190/p10-target'
ENV = os.environ.copy()
ENV['PATH'] = str(Path('C:/Users/tdotn/.cargo/bin')) + os.pathsep + str(ROOT / 'artifacts/tools/nextest') + os.pathsep + ENV['PATH']
ENV['CARGO_HOME'] = 'C:/Users/tdotn/.cargo'
ENV['RUSTUP_HOME'] = 'C:/Users/tdotn/.rustup'
ENV['CARGO_TARGET_DIR'] = str(TARGET)
ENV['CARGO_INCREMENTAL'] = '1'
ENV['AIRK_PRIVATE_CENSUS_BASE'] = str(ROOT / 'artifacts/v190/p10-private-fixtures-619c272e8a644988b26bbe4334fc8c02')
ENV['AIRK_BUNDLED_MODEL_DIR'] = str(ROOT / 'target/local-vector-model')
ENV['AIRK_BUNDLED_RUNTIME_DIR'] = str(ROOT / 'target/local-onnx-runtime')
ENV['AIRK_TEST_MODEL_DIR'] = str(ROOT / 'target/local-vector-model/minilm-l6-v2-u8-v1')
ENV['AIRK_TEST_BINARY'] = str(OUT / 'runtime-debug/airk.exe')
ENV['AIRK_CATALOG_CANDIDATE_BINARY'] = ENV['AIRK_TEST_BINARY']
for name in ('AIRK_P10_RESOURCE_OUTPUT_DIR', 'AIRK_P11B_RESOURCE_OUTPUT_DIR'):
    ENV.pop(name, None)

COMMANDS = {
    'provision-model': [sys.executable, 'scripts/provision-local-model.py', '--destination', ENV['AIRK_BUNDLED_MODEL_DIR'], '--download'],
    'provision-onnx': [sys.executable, 'scripts/provision-onnx-runtime.py', '--download'],
    'scoped-clippy': [sys.executable, 'scripts/dev-check.py', '-p', 'airk-core', '-p', 'airk-cli', '--target-dir', str(TARGET), '--incremental', 'on'],
    'workspace-clippy': [sys.executable, 'scripts/dev-check.py', '--workspace', '--all-features', '--target-dir', str(TARGET), '--incremental', 'on'],
    'lightweight': ['cargo', 'check', '-p', 'airk-cli', '--no-default-features', '--all-targets', '--locked', '--timings'],
    'runtime-debug': [sys.executable, 'scripts/build-service-runtime.py', '--destination', str(OUT / 'runtime-debug'), '--profile', 'debug'],
    'preflight': ['cargo', 'nextest', 'run', '--locked', '--offline', '--timings', '-p', 'airk-cli', '--test', 'catalog_upgrade', '--all-features', '--profile', 'ci', '--no-fail-fast', '--retries', '0', '--test-threads', '2'],
    'workspace-build': ['cargo', 'build', '--workspace', '--locked', '--timings'],
    'workspace-tests': ['cargo', 'nextest', 'run', '--workspace', '--all-features', '--locked', '--offline', '--timings', '--profile', 'ci', '--ignore-default-filter', '--no-fail-fast', '--retries', '0', '--test-threads', '2'],
    'doctests': ['cargo', 'test', '--doc', '--workspace', '--locked', '--timings', '--', '--test-threads=1'],
    'service-clients': [sys.executable, 'scripts/check-service-clients.py', '--runtime-dir', str(OUT / 'runtime-debug'), '--files', '16', '--model'],
    'previous-upgrade': [sys.executable, 'scripts/check-runtime-upgrade.py', '--runtime', str(OUT / 'runtime-debug'), '--previous-runtime', str(OUT / 'previous-1.8.8'), '--output', str(OUT / 'previous-upgrade-receipt.json')],
    'extension-tests': ['npm.cmd', 'test', '--prefix', 'packages/vscode'],
    'onboarding-tests': ['node', '--test', 'packages/vscode/out/test/serviceOnboarding.test.js'],
    'python-tests': [sys.executable, '-m', 'unittest', 'discover', '-s', 'scripts/tests', '-v'],
    'repo-check': ['pwsh', '-NoProfile', '-File', 'scripts/check-repo.ps1'],
    'notices': [sys.executable, 'scripts/license-notices.py', '--output', str(OUT / 'notices'), '--release'],
    'dependency-audit': [sys.executable, 'scripts/dependency-audit.py'],
    'runtime-release': [sys.executable, 'scripts/build-service-runtime.py', '--destination', str(OUT / 'runtime-release'), '--profile', 'release'],
    'package-native': [sys.executable, 'scripts/package-native.py', '--target', 'x86_64-pc-windows-msvc', '--runtime-dir', str(OUT / 'runtime-release'), '--output', str(OUT / 'native')],
    'package-vsix': [sys.executable, 'scripts/package-extension.py', '--target', 'x86_64-pc-windows-msvc', '--native-dir', str(OUT / 'native/publish'), '--output', str(OUT / 'vscode')],
    'whitespace': ['git', '-c', f'safe.directory={ROOT.as_posix()}', 'diff', '--check'],
}

for stage in sys.argv[1:]:
    cmd = COMMANDS.get(stage, COMMANDS.get(stage.rsplit('-r', 1)[0]))
    if cmd is None:
        raise SystemExit(stage)
    if cmd[0] == 'cargo':
        cmd = ['C:/Users/tdotn/.cargo/bin/cargo.exe', *cmd[1:]]
    candidate = OUT / 'runtime-debug/airk.exe'
    if candidate.is_file():
        ENV['AIRK_TEST_BINARY'] = str(candidate)
        ENV['AIRK_CATALOG_CANDIDATE_BINARY'] = str(candidate)
    else:
        ENV.pop('AIRK_TEST_BINARY', None)
        ENV.pop('AIRK_CATALOG_CANDIDATE_BINARY', None)
    receipt_path = OUT / (stage + '.json')
    if receipt_path.exists():
        raise SystemExit(f'Result already exists: {stage}; retain it and use an explicitly named new attempt')
    print(json.dumps({'stage': stage, 'command': cmd, 'target': str(TARGET), 'incremental': 1}), flush=True)
    started = time.monotonic()
    log = OUT / (stage + '.log')
    with log.open('w', encoding='utf-8') as stream:
        try:
            result = subprocess.run(cmd, cwd=ROOT, env=ENV, stdout=stream, stderr=subprocess.STDOUT)
            code = result.returncode
        except Exception as exc:
            stream.write(repr(exc))
            code = 127
    timings = []
    saved = OUT / 'cargo-timings' / stage
    saved.mkdir(parents=True, exist_ok=True)
    for report in (TARGET / 'cargo-timings').glob('*.html'):
        if report.stat().st_mtime >= time.time() - (time.monotonic() - started) - 2:
            shutil.copy2(report, saved / report.name)
            timings.append(str(saved / report.name))
    receipt = {'stage': stage, 'command': cmd, 'cwd': str(ROOT), 'target': str(TARGET), 'incremental': 1,
               'seconds': time.monotonic() - started, 'exit_code': code, 'log': str(log),
               'log_sha256': hashlib.sha256(log.read_bytes()).hexdigest(), 'timings': timings}
    receipt_path.write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(receipt), flush=True)
    if code:
        print(log.read_text(encoding='utf-8', errors='replace')[-12000:], flush=True)
        raise SystemExit(code)
