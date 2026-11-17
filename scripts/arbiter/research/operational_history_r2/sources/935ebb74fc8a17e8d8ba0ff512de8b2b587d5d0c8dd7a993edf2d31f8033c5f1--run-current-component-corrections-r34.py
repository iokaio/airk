# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
root=Path.cwd();env={k:v for k,v in os.environ.items() if not k.upper().startswith('GIT_')};env.update(json.loads((root/'artifacts/advanced-split/ort130-x64/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'artifacts/advanced-split/bundled-model-v2'))
selector='test(dated_source_receipts_normalize_only_valid_clocks_and_keep_hashes_and_future_data) | test(context_fallback_comparison_retains_evidence_and_rejects_orphaned_or_unearned_identity_changes) | test(current_answer_single_index_preserves_complete_pipe_cli_and_https_mcp_results) | test(current_answer_many_and_open_keep_complete_cli_pipe_and_https_mcp_packets) | test(admin_sign_in_reads_and_cross_plane_refusals)'
command=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,harness,fixtures,frontend','--out','artifacts/advanced-split/receipts/current-context-component-corrections-r34','--target-dir','artifacts/v190/p10-target','component-cli-mcp-admin','C:/Users/tdotn/.cargo/bin/cargo.exe','nextest','run','--profile','ci','-p','airk-cli','--no-default-features','--features','local-inference','--test','combined_reads','--test','admin_plane','--locked','--timings','-E',selector]
check=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,harness,fixtures,frontend','--out','artifacts/advanced-split/receipts/current-context-component-corrections-r34','--target-dir','artifacts/v190/p10-target','--allow-no-tests','scoped-cli-clippy','C:/Users/tdotn/.cargo/bin/cargo.exe','clippy','-p','airk-cli','--no-default-features','--features','local-inference','--all-targets','--locked','--timings','--','-D','warnings']
status=subprocess.run(check,env=env).returncode
if status:raise SystemExit(status)
raise SystemExit(subprocess.run(command,env=env).returncode)


