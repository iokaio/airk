# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ)
env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_bytes())['environment'])
env.update(CARGO_NET_OFFLINE='true',CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v2'))
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe';out=r/'artifacts/advanced-split/receipts/stage-integration-r98'
prefix=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native','--out',str(out),'--target-dir',str(r/'artifacts/v190/p10-target')]
def run(name,argv,extra=None):
 e=dict(env);e.update(extra or {});p=subprocess.run([*prefix,name,*argv],env=e)
 if p.returncode:raise SystemExit(p.returncode)
run('proof-controls',[cargo,'nextest','run','--profile','ci','-p','airk-service','--all-features','--locked','--timings','-E','test(service_runtime::arbiter::final_distribution::)'])
facts=r/'artifacts/advanced-split/models/selected-model-rights-r1/native-facts-input.json'
run('candidate-facts',[cargo,'nextest','run','--profile','ci','-p','airk-service','--all-features','--locked','--timings','-E','test(=service_runtime::arbiter::final_distribution::tests::emit_final_distribution_candidate_facts)','--run-ignored','only'],{'AIRK_FINAL_DISTRIBUTION_FACT_INPUT':str(facts),'AIRK_FINAL_DISTRIBUTION_FACT_INPUT_SHA256':'ec82750dd4f5ff72604858e2795eaa9fa919bb8e27eb762b53ca7a475fa43ba2','AIRK_FINAL_DISTRIBUTION_FACT_OUTPUT':str(out/'candidate-facts.json')})
run('portable-ui-cases',[cargo,'nextest','run','--profile','ci','-p','airk-core','-p','airkctl','--all-features','--locked','--timings','-E','test(=install_record::portable_contract_tests::portable_contract_has_explicit_ownership_and_no_implicit_os_integration) | test(=setup_receipt::tests::native_setup_observation_preserves_full_receipts_and_refuses_false_completion)'],{'AIRK_PORTABLE_CASE_OUTPUT':str(out/'portable-cases.json'),'AIRK_SETUP_RECEIPT_CASE_OUTPUT':str(out/'setup-receipt-cases.json')})
