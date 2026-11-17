# SPDX-License-Identifier: Apache-2.0
import os
import json
import subprocess
import sys
from pathlib import Path

env = {name:value for name,value in os.environ.items() if not name.upper().startswith('GIT_')}
env['CARGO_BUILD_JOBS'] = '8'
env['CARGO_INCREMENTAL'] = '1'
env['CARGO_NET_OFFLINE'] = 'true'
env.update(json.loads(Path('artifacts/advanced-split/ort130-x64/cargo-aggregate-env.json').read_text())['environment'])
env['AIRK_BUNDLED_MODEL_DIR'] = str(Path('artifacts/advanced-split/bundled-model-v2').resolve())
command = ['C:/Users/tdotn/.cargo/bin/cargo.exe', *sys.argv[1:]]
raise SystemExit(subprocess.call(command, env=env))
