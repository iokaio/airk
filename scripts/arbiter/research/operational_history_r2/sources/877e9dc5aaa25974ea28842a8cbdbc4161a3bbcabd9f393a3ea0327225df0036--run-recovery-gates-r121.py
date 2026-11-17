# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ)
env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_bytes())['environment'])
env.update(CARGO_NET_OFFLINE='true',CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v3'),AIRK_TEST_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v3/minilm-l6-v2-u8-v3'))
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe'
out=r/'artifacts/advanced-split/receipts/native-recovery-gates-r121'
common=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,fixtures,harness','--out',str(out),'--target-dir',str(r/'artifacts/v190/p10-target')]
commands=[
 [*common,'--allow-no-tests','service-clippy',cargo,'clippy','-p','airk-service','--all-targets','--all-features','--locked','--timings','--','-D','warnings'],
 [*common,'focused',cargo,'nextest','run','--profile','ci','-p','airk-service','-p','airk-supervisor','-p','airk-kernel','--all-features','--locked','--timings','-E','test(warm_observer::tests::) | test(runtime_metadata::arbiter::tests::sync_control_) | test(ready_context_retains_capacity_after_focus_reaches_its_cumulative_ceiling) | test(malformed_optional_permutation_is_not_delivered_or_replayed)'],
 [*common,'--allow-no-tests','actual-images',cargo,'test','-p','airk-service','-p','airk-supervisor','-p','airk-kernel','--all-features','--no-run','--message-format=json','--locked','--timings'],
]
for command in commands:
    result=subprocess.run(command,env=env)
    if result.returncode:raise SystemExit(result.returncode)
print(json.dumps({'complete':True,'receipt_directory':str(out),'expected_focused_tests':14}))
