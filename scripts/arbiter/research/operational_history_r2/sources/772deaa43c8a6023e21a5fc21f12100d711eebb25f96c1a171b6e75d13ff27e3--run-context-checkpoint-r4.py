# SPDX-License-Identifier: Apache-2.0
"""Scoped compiler/test checkpoint for original deadlines and allocation custody."""
import json
import os
from pathlib import Path
import subprocess
import sys

root = Path.cwd()
env = {key: value for key, value in os.environ.items() if not key.upper().startswith('GIT_')}
env.update(json.loads((root / 'artifacts/advanced-split/ort130-x64/cargo-aggregate-env.json').read_text())['environment'])
env.update(CARGO_BUILD_JOBS='8', CARGO_INCREMENTAL='1', CARGO_NET_OFFLINE='true',
           AIRK_BUNDLED_MODEL_DIR=str(root / 'artifacts/advanced-split/bundled-model-v2'))
cargo = 'C:/Users/tdotn/.cargo/bin/cargo.exe'
prefix = [sys.executable, 'scripts/dev/receipt_runner.py', '--groups', 'native,harness,fixtures',
          '--out', 'artifacts/advanced-split/receipts/native-context-r4',
          '--target-dir', 'artifacts/v190/p10-target']
commands = [
    ['scoped-clippy', cargo, 'clippy', '-p', 'airk-mcp', '-p', 'airk-supervisor', '-p', 'airkctl',
     '--all-targets', '--all-features', '--locked', '--timings', '--', '-D', 'warnings'],
    ['routed-deadline-tests', cargo, 'nextest', 'run', '--profile', 'ci', '-p', 'airk-mcp',
     '--all-features', '--locked', '--timings', '-E',
     'test(read_attempt::admission_tests::) | test(remote::parent_context_tests::)'],
]
for command in commands:
    result = subprocess.run(prefix + command, cwd=root, env=env)
    if result.returncode:
        raise SystemExit(result.returncode)
