# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
receipt=json.loads(Path('artifacts/local/one-airk-1.8.14/clean-break-r38-extension-compiled-before-r1.json').read_text())
env=dict(os.environ)
for key in ['AIRK_TEST_BINARY','AIRK_NATIVE_TEST_BINARY','AIRK_TEST_F09_QUALIFICATION']:env.pop(key,None)
print(json.dumps({'selected_test_files':len(receipt['test_paths']),'native_overrides':'removed'}),flush=True)
assert len(receipt['test_paths'])==108
sys.exit(subprocess.run([r'C:\Program Files\nodejs\node.exe','--test','--test-concurrency=2',*receipt['test_paths']],env=env).returncode)