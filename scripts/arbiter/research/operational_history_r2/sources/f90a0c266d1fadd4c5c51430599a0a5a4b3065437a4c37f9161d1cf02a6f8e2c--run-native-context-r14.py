# SPDX-License-Identifier: Apache-2.0
"""Frozen workspace compiler checkpoint before current joint-runtime integration."""
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
command = [sys.executable, 'scripts/dev/receipt_runner.py', '--groups', 'native,harness,fixtures,frontend',
           '--out', 'artifacts/advanced-split/receipts/native-context-r14',
           '--target-dir', 'artifacts/v190/p10-target', 'workspace-clippy',
           'C:/Users/tdotn/.cargo/bin/cargo.exe', 'clippy', '--workspace', '--all-targets',
           '--all-features', '--locked', '--timings', '--', '-D', 'warnings']
raise SystemExit(subprocess.run(command, cwd=root, env=env).returncode)








