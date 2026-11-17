# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,subprocess
r=Path.cwd();cmd=json.loads((r/'artifacts/advanced-split/ann-regeneration-r51-command.json').read_text());cmd.append('--execute-reviewed')
with (r/'artifacts/advanced-split/receipts/ann-regeneration-r51-root.log').open('xb') as log: result=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT)
print('native zero-NN regeneration exit',result.returncode)
raise SystemExit(result.returncode)
