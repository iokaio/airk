# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
root=Path.cwd();env={k:v for k,v in os.environ.items() if not k.upper().startswith('GIT_')};env.update(json.loads((root/'artifacts/advanced-split/ort130-x64/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'artifacts/advanced-split/bundled-model-v2'))
selector='test(current_answer_many_and_open_keep_complete_cli_pipe_and_https_mcp_packets)'
command=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,harness,fixtures,frontend','--out','artifacts/advanced-split/receipts/current-context-component-diagnostic-r2','--target-dir','artifacts/v190/p10-target','component-cli-mcp-admin','C:/Users/tdotn/.cargo/bin/cargo.exe','nextest','run','--profile','ci','-p','airk-cli','--no-default-features','--features','local-inference','--test','combined_reads','--test','admin_plane','--locked','--timings','-E',selector]
check=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,harness,fixtures,frontend','--out','artifacts/advanced-split/receipts/current-context-component-diagnostic-r2','--target-dir','artifacts/v190/p10-target','scoped-cli-clippy','C:/Users/tdotn/.cargo/bin/cargo.exe','clippy','-p','airk-cli','--no-default-features','--features','local-inference','--test','combined_reads','--locked','--timings','--','-D','warnings']
status=subprocess.run(check,env=env).returncode
if status:raise SystemExit(status)
raise SystemExit(subprocess.run(command,env=env).returncode)
