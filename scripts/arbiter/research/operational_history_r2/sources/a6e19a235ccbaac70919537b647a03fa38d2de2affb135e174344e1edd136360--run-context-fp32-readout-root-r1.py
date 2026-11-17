# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
p=Path.cwd()/'artifacts/advanced-split/models/qwen3-challengers-r1/context-fp32-readout-r1'
for label in ['17','4']:
 packet=json.loads((p/f'root-launch-command-{label}.json').read_bytes())
 with (p/f'root-launch-{label}.log').open('x') as log:result=subprocess.run(packet['launch_command'],stdout=log,stderr=subprocess.STDOUT)
 print(json.dumps({'label':label,'exit_code':result.returncode}),flush=True)
 if result.returncode:raise SystemExit(result.returncode)
