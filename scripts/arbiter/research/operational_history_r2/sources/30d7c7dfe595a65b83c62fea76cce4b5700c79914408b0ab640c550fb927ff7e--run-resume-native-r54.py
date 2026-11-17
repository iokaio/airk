# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ);env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v2'))
base=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,fixtures','--out','artifacts/advanced-split/receipts/resume-native-r54','--target-dir','artifacts/v190/p10-target'];cargo='C:/Users/tdotn/.cargo/bin/cargo.exe'
rows=[('combined-clippy',True,[cargo,'clippy','-p','airk-service','-p','airk-cli','-p','airk-kernel','-p','airk-retrieval','--all-targets','--all-features','--locked','--timings','--','-D','warnings']),('query-and-route-controls',False,[cargo,'nextest','run','--profile','ci','-p','airk-kernel','-p','airk-service','--lib','--all-features','--locked','--timings','--fail-fast','-E','test(query_only_gate_refuses_reorder_missing_refused_or_foreign_dev_tasks) | test(query_only_validates_every_native_request_shape_before_any_nn) | test(explicit_subset_controls_use_native_combined_owner_even_for_one_index)']),('actual-source-selectors',False,[cargo,'nextest','run','--profile','ci','-p','airk-cli','--test','combined_reads','--all-features','--locked','--timings','--fail-fast','-E','test(one_index_window_catalog_and_open_explicit_source_selectors_use_original_authorized_combined_owner)']),('capture-kernel-image',True,[cargo,'test','-p','airk-kernel','--lib','--all-features','--locked','--timings','--no-run','--message-format=json-render-diagnostics'])]
for label,allow,cmd in rows:
 result=subprocess.run(base+(['--allow-no-tests'] if allow else [])+[label]+cmd,env=env)
 if result.returncode:raise SystemExit(result.returncode)

