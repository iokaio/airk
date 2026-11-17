# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
root=Path.cwd();env=dict(os.environ);env.update(json.loads((root/'artifacts/advanced-split/ort130-x64/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_BUILD_JOBS='2',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'artifacts/advanced-split/bundled-model-v2'))
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe';base=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,harness,fixtures','--out','artifacts/advanced-split/receipts/native-preparation-r38','--target-dir','artifacts/v190/p10-target']
rows=[('kernel-clippy',True,[cargo,'clippy','-p','airk-kernel','--all-targets','--all-features','--locked','--timings','--','-D','warnings']),('pure-preparation',False,[cargo,'nextest','run','--profile','ci','-p','airk-kernel','--lib','--all-features','--locked','--timings','-E','test(native_proxy_fit_) | test(native_parent_coverage_requires_exact_union_without_cropping_units) | test(native_train_source_census_detects_extra_missing_and_alias_paths)']),('capture-kernel-image',True,[cargo,'test','-p','airk-kernel','--lib','--all-features','--locked','--timings','--no-run','--message-format=json-render-diagnostics'])]
for name,allow,cmd in rows:
 code=subprocess.run(base+(['--allow-no-tests'] if allow else [])+[name]+cmd,env=env).returncode
 if code:raise SystemExit(code)
