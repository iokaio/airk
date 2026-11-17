# SPDX-License-Identifier: Apache-2.0
"""Frozen focused Core contracts; semantic fixture has already been emitted."""
import os
from pathlib import Path
import subprocess
import sys
root = Path.cwd()
env = {key: value for key, value in os.environ.items() if not key.upper().startswith('GIT_')}
env.update(CARGO_BUILD_JOBS='8', CARGO_INCREMENTAL='1', CARGO_NET_OFFLINE='true')
command = [sys.executable, 'scripts/dev/receipt_runner.py', '--groups', 'native,harness,fixtures,frontend',
           '--out', 'artifacts/advanced-split/receipts/native-context-r16', '--target-dir',
           'artifacts/v190/p10-target', 'core-all-tests', 'C:/Users/tdotn/.cargo/bin/cargo.exe',
           'nextest', 'run', '--profile', 'ci', '-p', 'airk-core', '--lib', '--all-features',
           '--locked', '--timings']
raise SystemExit(subprocess.run(command, cwd=root, env=env).returncode)


