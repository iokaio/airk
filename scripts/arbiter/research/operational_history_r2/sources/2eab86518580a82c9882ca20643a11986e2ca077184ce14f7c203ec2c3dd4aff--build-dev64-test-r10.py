# SPDX-License-Identifier: Apache-2.0
"""Build/execute the synthetic DEV identity gate; no neural evaluation here."""
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
command = [sys.executable, 'scripts/dev/receipt_runner.py', '--groups', 'native,fixtures',
           '--out', 'artifacts/advanced-split/receipts/native-context-r10',
           '--target-dir', 'artifacts/v190/p10-target', 'dev64-identity-test',
           'C:/Users/tdotn/.cargo/bin/cargo.exe', 'nextest', 'run', '--profile', 'ci',
           '-p', 'airk-kernel', '--lib', '--features', 'test-support', '--locked', '--timings', '-E',
           'test(inference_owner::evaluation_tests::development::development_identity_gate_retains_all64_including44_refusals)']
raise SystemExit(subprocess.run(command, cwd=root, env=env).returncode)
