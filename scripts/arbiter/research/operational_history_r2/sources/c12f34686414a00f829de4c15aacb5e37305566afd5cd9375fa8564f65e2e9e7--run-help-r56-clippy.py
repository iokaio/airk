# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ);env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v2'))
cmd=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native','--out','artifacts/advanced-split/receipts/help-repair-r56','--target-dir','artifacts/v190/p10-target','--allow-no-tests','clippy','C:/Users/tdotn/.cargo/bin/cargo.exe','clippy','-p','airk-mcp','--all-targets','--all-features','--locked','--timings','--','-D','warnings']
raise SystemExit(subprocess.run(cmd,env=env).returncode)
