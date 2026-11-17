# SPDX-License-Identifier: Apache-2.0
import json, os, subprocess, sys
from pathlib import Path
base = Path(__file__).resolve().parent
root = base.parents[1]
corrected = json.loads((base / 'windows-r1/corrected-cases-r2.summary.json').read_text(encoding='utf-8'))
assert corrected['exit_code'] == 0 and not corrected['failed'] and corrected['passed'] == corrected['selected']
env = os.environ.copy()
env['AIRK_PRIVATE_CENSUS_BASE'] = str(root / 'artifacts/v190/p10-private-fixtures-619c272e8a644988b26bbe4334fc8c02')
env['AIRK_DEV_CARGO_INCREMENTAL'] = '1'
for name in ['AIRK_TEST_BINARY', 'AIRK_CATALOG_CANDIDATE_BINARY']:
    env.pop(name, None)
code = subprocess.run([sys.executable, '-X', 'utf8', str(base / 'command-r1.py'), 'full-native-r2.log', sys.executable, str(base / 'run-full-native-r2.py')], cwd=root, env=env).returncode
if code: sys.exit(code)
sys.exit(subprocess.run([sys.executable, '-X', 'utf8', str(base / 'run-final-rust-gates.py')], cwd=root, env=env).returncode)
