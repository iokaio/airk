# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ)
env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_bytes())['environment'])
env.update(CARGO_NET_OFFLINE='true',CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1');env.pop('AIRK_BUNDLED_MODEL_DIR',None)
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe';out=r/'artifacts/advanced-split/receipts/stage-integration-r99'
prefix=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native','--out',str(out),'--target-dir',str(r/'artifacts/v190/p10-target')]
def run(name,argv,extra=None):
 e=dict(env);e.update(extra or {});p=subprocess.run([*prefix,name,*argv],env=e)
 if p.returncode:raise SystemExit(p.returncode)
run('retrieval-clippy',[cargo,'clippy','-p','airk-retrieval','--all-targets','--all-features','--locked','--timings','--','-D','warnings'])
run('original-catalog-emitter',[cargo,'nextest','run','--profile','ci','-p','airk-retrieval','--all-features','--locked','--timings','-E','test(=model_catalog::baseline_emitter::emit_original_baseline_current_runtime_catalog_and_bundle_draft)','--run-ignored','only'],{'AIRK_ORIGINAL_MINILM_ASSETS':str(r/'artifacts/advanced-split/models/minilm-shipped-baseline-ort130-r1'),'AIRK_ORIGINAL_MINILM_DRAFT_OUTPUT':str(r/'artifacts/advanced-split/models/native-minilm-original-baseline-r1/native-draft-r1')})
