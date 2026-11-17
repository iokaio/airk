# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
root=Path.cwd();env=dict(os.environ);env.update(json.loads((root/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'artifacts/advanced-split/bundled-model-v2'))
cmd=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native-libraries,harness,fixtures','--out','artifacts/advanced-split/receipts/native-image-r42','--target-dir','artifacts/v190/p10-target','--allow-no-tests','compile-kernel-image','C:/Users/tdotn/.cargo/bin/cargo.exe','test','-p','airk-kernel','--lib','--all-features','--locked','--timings','--no-run','--message-format=json-render-diagnostics']
raise SystemExit(subprocess.run(cmd,env=env).returncode)


