# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ)
env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_bytes())['environment'])
env.update(CARGO_NET_OFFLINE='true',CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v2'))
out=r/'artifacts/advanced-split/receipts/stage-integration-r98'
env.update(AIRK_FINAL_DISTRIBUTION_FACT_INPUT=str(r/'artifacts/advanced-split/models/selected-model-rights-r1/native-facts-input.json'),AIRK_FINAL_DISTRIBUTION_FACT_INPUT_SHA256='ec82750dd4f5ff72604858e2795eaa9fa919bb8e27eb762b53ca7a475fa43ba2',AIRK_FINAL_DISTRIBUTION_FACT_OUTPUT=str(out/'candidate-facts.native-r2.json'))
command=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native','--out',str(out),'--target-dir',str(r/'artifacts/v190/p10-target'),'candidate-facts-emitter-r2','C:/Users/tdotn/.cargo/bin/cargo.exe','nextest','run','--profile','ci','-p','airk-service','--all-features','--locked','--timings','-E','test(=service_runtime::arbiter::final_distribution::tests::emit_final_distribution_candidate_facts)','--run-ignored','only']
raise SystemExit(subprocess.run(command,env=env).returncode)
