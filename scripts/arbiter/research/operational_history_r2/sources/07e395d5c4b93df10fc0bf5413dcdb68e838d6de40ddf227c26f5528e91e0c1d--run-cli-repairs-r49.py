# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ);env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v2'))
base=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,fixtures','--out','artifacts/advanced-split/receipts/cli-fixture-repairs-r49','--target-dir','artifacts/v190/p10-target'];cargo='C:/Users/tdotn/.cargo/bin/cargo.exe'
rows=[('cli-clippy',True,[cargo,'clippy','-p','airkctl','-p','airk-cli','--all-targets','--all-features','--locked','--timings','--','-D','warnings']),('scoped-discovery-control',False,[cargo,'nextest','run','--profile','ci','-p','airkctl','--lib','--all-features','--locked','--timings','--fail-fast','-E','test(fixture_discovery_is_thread_local_and_restores_nested_panic)']),('cli-clock-and-refusal',False,[cargo,'nextest','run','--profile','ci','-p','airk-cli','--test','combined_reads','--test','error_outcomes','--all-features','--locked','--timings','--fail-fast','-E','test(current_combined_remaining_clocks_preserve_all_fixed_caps_and_unknown_fields) | test(airkctl_combined_reads_match_current_native_handlers) | test(cli_service_refusal_preserves_locked_legacy_storage) | test(dated_source_receipts_normalize_only_valid_clocks_and_keep_hashes_and_future_data)'])]
for label,allow,cmd in rows:
 result=subprocess.run(base+(['--allow-no-tests'] if allow else [])+[label]+cmd,env=env)
 if result.returncode:raise SystemExit(result.returncode)
