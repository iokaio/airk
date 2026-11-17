# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
root=Path.cwd();env={k:v for k,v in os.environ.items() if not k.upper().startswith('GIT_')}
env.update(json.loads((root/'artifacts/advanced-split/ort130-x64/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'artifacts/advanced-split/bundled-model-v2'))
base=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,harness,fixtures,frontend','--out','artifacts/advanced-split/receipts/native-context-r22','--target-dir','artifacts/v190/p10-target'];cargo='C:/Users/tdotn/.cargo/bin/cargo.exe'
selector='package(airk-core) | package(airk-mcp) | (package(airk-kernel) & (test(arbiter::tests::) | test(arbiter::joint::tests::) | test(combined_reads::tests::context::) | test(inference_owner::context_action_tests::) | test(candidate_identity_gate) | test(fresh_final_population)))'
commands=[['workspace-clippy',cargo,'clippy','--workspace','--all-targets','--all-features','--locked','--timings','--','-D','warnings'],['core-mcp-joint-regressions',cargo,'nextest','run','--profile','ci','-p','airk-core','-p','airk-mcp','-p','airk-kernel','--lib','--all-features','--locked','--timings','-E',selector]]
for command in commands:
 status=subprocess.run(base+command,env=env).returncode
 if status:raise SystemExit(status)
