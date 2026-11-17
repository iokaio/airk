# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
root=Path.cwd();env={k:v for k,v in os.environ.items() if not k.upper().startswith('GIT_')}
env.update(json.loads((root/'artifacts/advanced-split/ort130-x64/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'artifacts/advanced-split/bundled-model-v2'))
base=[sys.executable,'scripts/dev/receipt_runner.py','--out','artifacts/advanced-split/receipts/native-context-r21','--target-dir','artifacts/v190/p10-target'];cargo='C:/Users/tdotn/.cargo/bin/cargo.exe'
rows=[('native,fixtures','workspace-clippy',None,[cargo,'clippy','--workspace','--all-targets','--all-features','--locked','--timings','--','-D','warnings']),('native','combined-semantic-emission','AIRK_WRITE_COMBINED_CONTEXT_CASES',[cargo,'nextest','run','--profile','ci','-p','airk-kernel','--lib','--all-features','--locked','--timings','-E','test(emit_combined_context_semantic_cases)']),('native','public-answer-semantic-emission','AIRK_WRITE_ARBITER_PUBLIC_ANSWER_CASES',[cargo,'test','-p','airk-mcp','--lib','--all-features','--locked','--timings','arbiter_public_tests::public_answer_sdk_preserves_advertised_schema_full_utf8_question_and_budget'])]
for groups,name,variable,cmd in rows:
    child=dict(env)
    if variable:child[variable]='1'
    status=subprocess.run(base+['--groups',groups,name]+cmd,env=child).returncode
    if status:raise SystemExit(status)
