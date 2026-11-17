# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys,hashlib
from pathlib import Path
r=Path.cwd();env=dict(os.environ);env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v2'))
base=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,fixtures','--tier','T3','--out','artifacts/advanced-split/receipts/workspace-tests-r47','--target-dir','artifacts/v190/p10-target'];cargo='C:/Users/tdotn/.cargo/bin/cargo.exe'
cfg=r/'.config/nextest.toml';pin=hashlib.sha256(cfg.read_bytes()).hexdigest()
cmd=[cargo,'nextest','run','--profile','ci','--workspace','--all-features','--locked','--timings','--ignore-default-filter']
result=subprocess.run(base+['workspace-all-features']+cmd,env=env)
assert hashlib.sha256(cfg.read_bytes()).hexdigest()==pin,'Nextest config changed'
print('Nextest configuration SHA256',pin,flush=True)
raise SystemExit(result.returncode)
