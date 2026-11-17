# SPDX-License-Identifier: Apache-2.0
"""Capture hypothetical Core semantic cases through their real Rust producer."""
import os
from pathlib import Path
import subprocess
import sys

root = Path.cwd()
output = root / 'fixtures/split/arbiter/context_action_cases.json'
assert not output.exists(), 'Do not overwrite an earlier semantic capture'
env = {key: value for key, value in os.environ.items() if not key.upper().startswith('GIT_')}
env.update(CARGO_BUILD_JOBS='8', CARGO_INCREMENTAL='1', CARGO_NET_OFFLINE='true',
           AIRK_WRITE_CONTEXT_ACTION_CASES='1')
command = [sys.executable, 'scripts/dev/receipt_runner.py', '--groups', 'native,harness',
           '--out', 'artifacts/advanced-split/receipts/native-context-r3',
           '--target-dir', 'artifacts/v190/p10-target', 'core-semantic-emission',
           'C:/Users/tdotn/.cargo/bin/cargo.exe', 'nextest', 'run', '--profile', 'ci',
           '-p', 'airk-core', '--lib', '--all-features', '--locked', '--timings',
           '-E', 'test(emit_context_action_semantic_cases)']
raise SystemExit(subprocess.run(command, cwd=root, env=env).returncode)
