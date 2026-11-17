# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ);env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v2'))
base=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,frontend,harness,fixtures','--out','artifacts/advanced-split/receipts/workspace-r67','--target-dir','artifacts/v190/p10-target'];cargo='C:/Users/tdotn/.cargo/bin/cargo.exe'
rows=[('clippy',True,[cargo,'clippy','--workspace','--all-targets','--all-features','--locked','--timings','--','-D','warnings']),('nextest',False,[cargo,'nextest','run','--profile','ci','--workspace','--all-features','--locked','--timings','--ignore-default-filter','--fail-fast']),('doctests',False,[cargo,'test','--doc','--workspace','--all-features','--locked','--timings'])]
for label,allow,cmd in rows:
 result=subprocess.run(base+(['--allow-no-tests'] if allow else [])+[label]+cmd,env=env)
 if result.returncode:raise SystemExit(result.returncode)
