# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
root=Path.cwd();env={k:v for k,v in os.environ.items() if not k.upper().startswith('GIT_')};env.update(json.loads((root/'artifacts/advanced-split/ort130-x64/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'artifacts/advanced-split/bundled-model-v2'))
assert not (root/'artifacts/advanced-split/runtime-current-context-r1').exists()
cmd=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,harness,fixtures,frontend','--tier','T2','--out','artifacts/advanced-split/receipts/current-context-pair-build-r1','--target-dir','artifacts/v190/p10-target','--node',str(root/'artifacts/tools/node-v24.15.0-win-x64/node.exe'),'--allow-no-tests','matched-pair-build',sys.executable,'scripts/build-service-runtime.py','--destination','artifacts/advanced-split/runtime-current-context-r1','--profile','debug','--admin-inventory','artifacts/admin/admin-assets.json']
raise SystemExit(subprocess.run(cmd,env=env).returncode)
