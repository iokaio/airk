# SPDX-License-Identifier: Apache-2.0
"""Core-only contract checks before changing native consumers."""
import json
import os
from pathlib import Path
import subprocess
import sys

root = Path.cwd()
out = root / 'artifacts/advanced-split/receipts/core-read-attempt-contract-r1'
env = {key: value for key, value in os.environ.items() if not key.upper().startswith('GIT_')}
env.update(CARGO_INCREMENTAL='1', CARGO_BUILD_JOBS='8', CARGO_NET_OFFLINE='true')
cargo = 'C:/Users/tdotn/.cargo/bin/cargo.exe'
target = str(root / 'artifacts/v190/p10-target')
prefix = [sys.executable, 'scripts/dev/receipt_runner.py', '--groups', 'native,harness,fixtures',
          '--out', str(out), '--target-dir', target]
for name, command in [
    ('core-clippy', [sys.executable, 'scripts/dev-check.py', '-p', 'airk-core', '--target-dir', target,
                     '--incremental', 'on', '--offline', '--cargo', cargo]),
    ('core-read-attempt-tests', [cargo, 'nextest', 'run', '--profile', 'ci', '-p', 'airk-core', '--lib',
                                '--locked', '--timings', '-E', 'test(read_attempt::)']),
]:
    status = subprocess.run([*prefix, name, *command], cwd=root, env=env).returncode
    if status:
        raise SystemExit(status)
