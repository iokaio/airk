# SPDX-License-Identifier: Apache-2.0
import json,subprocess,sys
from pathlib import Path
receipt=json.loads(Path('artifacts/local/one-airk-1.8.14/clean-break-r38-extension-compiled-before-r2.json').read_text())
print(json.dumps({'selected_test_files':len(receipt['test_paths']),'native_runtime':'original sealed R37, native overrides enabled'}),flush=True)
assert len(receipt['test_paths'])==108
sys.exit(subprocess.run([r'C:\Program Files\nodejs\node.exe','--test','--test-concurrency=2',*receipt['test_paths']]).returncode)