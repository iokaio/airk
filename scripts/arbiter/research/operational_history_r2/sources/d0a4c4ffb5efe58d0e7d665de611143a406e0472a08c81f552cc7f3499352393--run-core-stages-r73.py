# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ);env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v2'))
base=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native','--out','artifacts/advanced-split/receipts/core-stages-r73','--target-dir','artifacts/v190/p10-target'];cargo='C:/Users/tdotn/.cargo/bin/cargo.exe'
rows=[('clippy',True,[cargo,'clippy','-p','airk-core','--all-targets','--all-features','--locked','--timings','--','-D','warnings']),('core-stage-cases',False,[cargo,'nextest','run','--profile','ci','-p','airk-core','--all-features','--locked','--timings','--fail-fast','-E','test(arbiter::stage_tests::) | test(arbiter::focus::tests::) | test(native_focus::tests::)'])]
for label,allow,cmd in rows[:1]:
 result=subprocess.run(base+(['--allow-no-tests'] if allow else [])+[label]+cmd,env=env)
 if result.returncode:raise SystemExit(result.returncode)
