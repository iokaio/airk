# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
receipt=json.loads(Path('artifacts/local/one-airk-1.8.14/clean-break-r39-extension-inputs-before-r1.json').read_text())
assert len(receipt['test_paths'])==108
env=dict(os.environ)
for name in ['AIRK_TEST_BINARY','AIRK_NATIVE_TEST_BINARY','AIRK_TEST_F09_QUALIFICATION']:env.pop(name,None)
print(json.dumps({'selected_test_modules':108}),flush=True)
sys.exit(subprocess.run([r'C:\Program Files\nodejs\node.exe','--test','--test-concurrency=2',*receipt['test_paths']],env=env).returncode)