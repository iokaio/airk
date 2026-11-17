# SPDX-License-Identifier: Apache-2.0
"""Actual Rust outputs, ordered before consumers of the emitted semantic cases."""
import os
from pathlib import Path
import subprocess
import sys
root = Path.cwd()
out = root / 'artifacts/advanced-split/receipts/native-context-r16'
out.mkdir(parents=True, exist_ok=True)
env = {key: value for key, value in os.environ.items() if not key.upper().startswith('GIT_')}
env.update(CARGO_BUILD_JOBS='8', CARGO_INCREMENTAL='1', CARGO_NET_OFFLINE='true')
prefix = [sys.executable, 'scripts/dev/receipt_runner.py', '--groups', 'native,harness,frontend',
          '--out', str(out), '--target-dir', 'artifacts/v190/p10-target']
for name, selector, variable, filename in [
    ('context-semantic-emission', 'emit_context_action_semantic_cases', 'AIRK_WRITE_CONTEXT_ACTION_CASES', 'context_action_cases.json'),
    ('native-semantic-emission', 'emit_native_context_action_cases', 'AIRK_WRITE_NATIVE_CONTEXT_ACTION_CASES', 'native_context_action_cases.json'),
]:
    source = root / 'fixtures/split/arbiter' / filename
    if source.exists():
        with (out / (filename + '.before')).open('xb') as backup:
            backup.write(source.read_bytes())
    child = {**env, variable: '1'}
    command = [name, 'C:/Users/tdotn/.cargo/bin/cargo.exe', 'nextest', 'run', '--profile', 'ci',
               '-p', 'airk-core', '--lib', '--all-features', '--locked', '--timings', '-E', f'test({selector})']
    result = subprocess.run(prefix + command, cwd=root, env=child)
    if result.returncode:
        raise SystemExit(result.returncode)


