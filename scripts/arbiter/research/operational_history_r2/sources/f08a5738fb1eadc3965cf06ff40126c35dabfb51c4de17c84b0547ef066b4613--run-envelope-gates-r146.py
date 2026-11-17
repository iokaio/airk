# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ)
env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_bytes())['environment'])
env.update(CARGO_NET_OFFLINE='true',CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v3'),AIRK_TEST_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v3/minilm-l6-v2-u8-v3'))
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe';out=r/'artifacts/advanced-split/receipts/envelope-gates-r146'
assert not out.exists()
base=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,fixtures,harness','--out',str(out),'--target-dir',str(r/'artifacts/v190/p10-target')]
packages=['-p','airk-kernel','-p','airk-supervisor','-p','airk-cli']
commands=[
 ('clippy',None,[*base,'--allow-no-tests','clippy',cargo,'clippy',*packages,'--all-targets','--all-features','--locked','--timings','--','-D','warnings']),
 ('pure',4,[*base,'pure',cargo,'nextest','run','--profile','ci',*packages,'--all-features','--locked','--timings','-E','test(project_worker::response_tests::) | test(context_byte_admission_charges_closed_envelope_escaping_and_original_metadata)']),
 ('actual-combined',1,[*base,'actual-combined',cargo,'nextest','run','--profile','ci','-p','airk-cli','--all-features','--test','combined_reads','--locked','--timings','-E','test(current_answer_many_and_open_keep_complete_cli_pipe_and_https_mcp_packets)']),
]
for name,count,command in commands:
    result=subprocess.run(command,env=env)
    if result.returncode:raise SystemExit(result.returncode)
    if count is not None:assert json.loads((out/(name+'.json')).read_bytes())['selected_tests']==count
print(json.dumps({'complete':True,'selected_pure':4,'selected_actual':1}))
