# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ);env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v2'))
cmd=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,frontend,harness,fixtures','--out','artifacts/advanced-split/receipts/workspace-r62','--target-dir','artifacts/v190/p10-target','tests','C:/Users/tdotn/.cargo/bin/cargo.exe','test','--workspace','--all-features','--locked','--timings']
raise SystemExit(subprocess.run(cmd,env=env).returncode)
