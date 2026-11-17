# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ);env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v2'))
base=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,fixtures','--out','artifacts/advanced-split/receipts/workspace-clippy-r46','--target-dir','artifacts/v190/p10-target','--allow-no-tests']
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe'
for label,cmd in [('lightweight-cli',[cargo,'clippy','-p','airk-cli','--no-default-features','--all-targets','--locked','--timings','--','-D','warnings']),('workspace-all-features',[cargo,'clippy','--workspace','--all-targets','--all-features','--locked','--timings','--','-D','warnings'])]:
 result=subprocess.run(base+[label]+cmd,env=env)
 if result.returncode:raise SystemExit(result.returncode)

