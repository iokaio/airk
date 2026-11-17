# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
root=Path.cwd(); env={k:v for k,v in os.environ.items() if not k.upper().startswith('GIT_')}
env.update(json.loads((root/'artifacts/advanced-split/ort130-x64/cargo-aggregate-env.json').read_text())['environment'])
env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'artifacts/advanced-split/bundled-model-v2'))
env['AIRK_WRITE_ARBITER_PUBLIC_ANSWER_CASES']='1'
command=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,harness,frontend','--out','artifacts/advanced-split/receipts/native-context-r16','--target-dir','artifacts/v190/p10-target','public-answer-semantic-emission','C:/Users/tdotn/.cargo/bin/cargo.exe','test','-p','airk-mcp','--lib','--all-features','--locked','--timings','arbiter_public_tests::public_answer_sdk_preserves_advertised_schema_full_utf8_question_and_budget']
raise SystemExit(subprocess.run(command,env=env).returncode)
