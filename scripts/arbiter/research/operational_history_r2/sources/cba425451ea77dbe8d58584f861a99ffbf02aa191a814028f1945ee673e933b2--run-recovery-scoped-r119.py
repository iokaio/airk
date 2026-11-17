# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ)
env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_bytes())['environment'])
env.update(CARGO_NET_OFFLINE='true',CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v3'),AIRK_TEST_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v3/minilm-l6-v2-u8-v3'))
command=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,fixtures,harness','--out',str(r/'artifacts/advanced-split/receipts/native-recovery-scoped-r119'),'--target-dir',str(r/'artifacts/v190/p10-target'),'--allow-no-tests','service-clippy','C:/Users/tdotn/.cargo/bin/cargo.exe','clippy','-p','airk-service','--all-targets','--all-features','--locked','--timings','--','-D','warnings']
raise SystemExit(subprocess.run(command,env=env).returncode)
