# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ)
env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_bytes())['environment'])
env.update(CARGO_NET_OFFLINE='true',CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v3'),GIT_CONFIG_COUNT='1',GIT_CONFIG_KEY_0='safe.directory',GIT_CONFIG_VALUE_0=str(r))
env['PATH']=str(r/'artifacts/tools/node-v24.15.0-win-x64')+';C:/Users/tdotn/.cargo/bin;'+env['PATH']
out=r/'artifacts/advanced-split/receipts/runtime-pair-r104';destination=r/'artifacts/advanced-split/runtime-selected-baseline-r104'
assert not destination.exists()
command=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native','--out',str(out),'--target-dir',str(r/'artifacts/v190/p10-target'),'--allow-no-tests','runtime-build',sys.executable,'scripts/build-service-runtime.py','--destination',str(destination),'--profile','debug','--admin-inventory',str(r/'artifacts/advanced-split/receipts/final-help-resume-r2/admin-assets.json')]
raise SystemExit(subprocess.run(command,env=env).returncode)
