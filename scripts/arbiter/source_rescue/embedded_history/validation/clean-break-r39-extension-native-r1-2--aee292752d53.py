# SPDX-License-Identifier: Apache-2.0
import json,subprocess,sys
from pathlib import Path
receipt=json.loads(Path('artifacts/local/one-airk-1.8.14/clean-break-r39-extension-inputs-before-r1.json').read_text())
assert len(receipt['test_paths'])==108
print(json.dumps({'selected_test_files':108,'native_runtime':'fresh sealed R39, native overrides enabled'}),flush=True)
sys.exit(subprocess.run([r'C:\Program Files\nodejs\node.exe','--test','--test-concurrency=2',*receipt['test_paths']]).returncode)