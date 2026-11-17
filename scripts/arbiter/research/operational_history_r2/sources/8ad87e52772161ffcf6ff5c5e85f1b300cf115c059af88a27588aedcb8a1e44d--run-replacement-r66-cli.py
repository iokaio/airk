# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ);env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v2'))
cmd=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,fixtures','--out','artifacts/advanced-split/receipts/replacement-recovery-r66','--target-dir','artifacts/v190/p10-target','actual-cli','C:/Users/tdotn/.cargo/bin/cargo.exe','nextest','run','--profile','ci','-p','airk-cli','--test','cli_commands','--all-features','--locked','--timings','--fail-fast','-E','test(same_build_cli_replacement_removes_saved_indexes_and_reactivation_starts_empty) | test(standalone_uninstall_without_confirmation_preserves_the_explicit_installation) | test(replacement_installation_flags_are_enforced_and_fresh_admission_replays_without_state)']
raise SystemExit(subprocess.run(cmd,env=env).returncode)
