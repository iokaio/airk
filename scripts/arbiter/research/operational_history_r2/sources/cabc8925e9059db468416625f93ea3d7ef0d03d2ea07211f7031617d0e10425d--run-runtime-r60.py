# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ);env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v2'))
cmd=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,frontend,harness','--out','artifacts/advanced-split/receipts/runtime-r60','--target-dir','artifacts/v190/p10-target','--allow-no-tests','build-pair',sys.executable,'scripts/build-service-runtime.py','--destination','artifacts/advanced-split/runtime-current-context-r3','--profile','debug','--admin-inventory','artifacts/advanced-split/admin-source-r59/admin-assets.json']
raise SystemExit(subprocess.run(cmd,env=env).returncode)
