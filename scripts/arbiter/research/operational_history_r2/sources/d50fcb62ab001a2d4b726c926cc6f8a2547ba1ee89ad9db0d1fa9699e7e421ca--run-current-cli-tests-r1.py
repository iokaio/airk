# SPDX-License-Identifier: Apache-2.0
"""Focused native deadline/allowance regressions; no model campaign selected."""
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
selection = 'test(current_catalog_rejects_changed_or_unknown_definitions) | test(airkctl_combined_reads_match_current_native_handlers)'
command = [sys.executable, 'scripts/dev/receipt_runner.py', '--groups', 'native,harness,fixtures,frontend',
           '--out', 'artifacts/advanced-split/receipts/native-context-r3', '--target-dir',
           'artifacts/v190/p10-target', 'current-cli-handler-tests', 'C:/Users/tdotn/.cargo/bin/cargo.exe',
           'nextest', 'run', '--profile', 'ci', '--test', 'combined_reads', '--all-features', '--locked', '--timings']
for package in ['airk-cli']:
    command.extend(['-p', package])
command.extend(['-E', selection])
raise SystemExit(subprocess.run(command, cwd=root, env=env).returncode)
