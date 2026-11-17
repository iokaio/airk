# SPDX-License-Identifier: Apache-2.0
"""Run the owning matched-pair producer on frozen local candidate inputs."""
import datetime
import json
import os
from pathlib import Path
import subprocess
import sys
import time

root = Path.cwd()
env = {k: v for k, v in os.environ.items() if not k.upper().startswith('GIT_')}
env.update(json.loads((root / 'artifacts/advanced-split/ort130-x64/cargo-aggregate-env.json').read_text())['environment'])
env.update(CARGO_BUILD_JOBS='8', CARGO_INCREMENTAL='1', CARGO_NET_OFFLINE='true',
           CARGO_TARGET_DIR=str(root / 'artifacts/v190/p10-target'),
           AIRK_BUNDLED_MODEL_DIR=str(root / 'artifacts/advanced-split/bundled-model-v2'))
env['PATH'] = str(Path('C:/Users/tdotn/.cargo/bin')) + os.pathsep + env['PATH']
command = [sys.executable, 'scripts/build-service-runtime.py', '--destination',
           'artifacts/advanced-split/runtime-debug-r1', '--profile', 'debug',
           '--admin-inventory', 'artifacts/admin/admin-assets.json']
receipt = {'command': command, 'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
           'scope': 'Local experimental candidate; A1 quality failed. No release qualification.',
           'target_directory': env['CARGO_TARGET_DIR'], 'incremental': True}
start = time.monotonic()
result = subprocess.run(command, env=env)
receipt.update(returncode=result.returncode, elapsed_seconds=time.monotonic()-start,
               finished_utc=datetime.datetime.now(datetime.timezone.utc).isoformat())
(root / 'artifacts/advanced-split/receipts/delivery-09-runtime-debug-r1.json').write_text(json.dumps(receipt, indent=2)+'\n')
raise SystemExit(result.returncode)
