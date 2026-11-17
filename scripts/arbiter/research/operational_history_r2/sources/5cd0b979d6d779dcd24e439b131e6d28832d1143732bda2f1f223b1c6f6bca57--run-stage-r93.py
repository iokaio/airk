# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ)
env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_bytes())['environment'])
env.update(CARGO_NET_OFFLINE='true',CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v2'))
cmd=json.loads((r/'artifacts/advanced-split/receipts/stage-integration-r88/focused.json').read_bytes())['command']
prefix=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native','--out',str(r/'artifacts/advanced-split/receipts/stage-integration-r93'),'--target-dir',str(r/'artifacts/v190/p10-target')]
repro=[cmd[0],'nextest','run','--profile','ci','-p','airk-kernel','-p','airk-retrieval','--all-features','--locked','--timings','-E','test(direct_answer_entry_binds_actual_parent_before_its_focus_search) | test(embedded_current_catalog_is_observable_without_starting_or_qualifying_models)']
for name,command in [('reproductions',repro),('focused',cmd)]:
 result=subprocess.run([*prefix,name,*command],env=env)
 if result.returncode:raise SystemExit(result.returncode)
