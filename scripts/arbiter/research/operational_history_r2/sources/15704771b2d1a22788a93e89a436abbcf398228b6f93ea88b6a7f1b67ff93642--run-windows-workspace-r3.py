# SPDX-License-Identifier: Apache-2.0
"""Reviewed Windows gate on frozen native/harness/fixture inputs; stop on failure."""
import datetime
import json
import os
from pathlib import Path
import subprocess
import sys
import time

root = Path.cwd()
destination = root / 'artifacts/advanced-split/receipts/windows-workspace-r3'
destination.mkdir(exist_ok=False)
env = {key: value for key, value in os.environ.items() if not key.upper().startswith('GIT_')}
env.update(json.loads((root / 'artifacts/advanced-split/ort130-x64/cargo-aggregate-env.json').read_text())['environment'])
env.update(CARGO_BUILD_JOBS='8', CARGO_INCREMENTAL='1', CARGO_NET_OFFLINE='true',
           CARGO_TARGET_DIR=str(root / 'artifacts/v190/p10-target'),
           AIRK_BUNDLED_MODEL_DIR=str(root / 'artifacts/advanced-split/bundled-model-v2'))
cargo = 'C:/Users/tdotn/.cargo/bin/cargo.exe'
prefix = [sys.executable, 'scripts/dev/receipt_runner.py', '--groups', 'native,harness,fixtures',
          '--tier', 'T3', '--out', str(destination), '--target-dir', env['CARGO_TARGET_DIR']]
stages = [
    ('workspace-clippy', [cargo, 'clippy', '--workspace', '--all-targets', '--all-features', '--locked', '--timings', '--', '-D', 'warnings']),
    ('workspace-tests', [cargo, 'test', '--workspace', '--all-features', '--locked', '--timings']),
    ('workspace-doctests', [cargo, 'test', '--doc', '--workspace', '--locked', '--timings']),
]
record = {'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'scope': 'Windows all-feature workspace gate; not other-platform, model-quality, installed or release qualification',
          'target_directory': env['CARGO_TARGET_DIR'], 'incremental': True, 'stages': []}
status = 0
for name, command in stages:
    started = time.monotonic()
    result = subprocess.run([*prefix, name, *command], cwd=root, env=env)
    record['stages'].append({'name': name, 'command': command, 'exit_code': result.returncode,
                             'elapsed_seconds': time.monotonic() - started})
    status = result.returncode
    (destination / 'campaign.json').write_text(json.dumps(record, indent=2) + '\n')
    if status:
        break
record['finished_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
(destination / 'campaign.json').write_text(json.dumps(record, indent=2) + '\n')
raise SystemExit(status)
