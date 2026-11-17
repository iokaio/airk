# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ);env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v2'))
cmd=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,fixtures','--out','artifacts/advanced-split/receipts/replacement-recovery-r66','--target-dir','artifacts/v190/p10-target','controls','C:/Users/tdotn/.cargo/bin/cargo.exe','nextest','run','--profile','ci','-p','airk-store','-p','airk-installer','--lib','--all-features','--locked','--timings','--fail-fast','-E','test(stopped_registry_) | test(replacement_recovers_stopped_hot_journal) | test(replacement_live_registry_owner) | test(replacement_resume_recovers_unfrozen_hot_journal)']
raise SystemExit(subprocess.run(cmd,env=env).returncode)
