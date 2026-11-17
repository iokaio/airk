# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ);env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v2'))
base=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,fixtures','--out','artifacts/advanced-split/receipts/mcp-catalog-r69','--target-dir','artifacts/v190/p10-target'];cargo='C:/Users/tdotn/.cargo/bin/cargo.exe'
rows=[('clippy',True,[cargo,'clippy','-p','airk-cli','--all-targets','--all-features','--locked','--timings','--','-D','warnings']),('actual-catalog',False,[cargo,'nextest','run','--profile','ci','-p','airk-cli','--test','tools','--test','service_runtime','--all-features','--locked','--timings','--fail-fast','-E','test(tools_help_works_without_a_workspace_and_exposes_live_schemas) | test(reviewed_https_discovery_and_evidence_preserve_native_contracts)'])]
for label,allow,cmd in rows:
 result=subprocess.run(base+(['--allow-no-tests'] if allow else [])+[label]+cmd,env=env)
 if result.returncode:raise SystemExit(result.returncode)
