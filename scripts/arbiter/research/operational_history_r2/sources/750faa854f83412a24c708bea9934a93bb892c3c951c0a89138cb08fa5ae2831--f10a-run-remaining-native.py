# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,subprocess,sys
root=Path.cwd();a=root/'artifacts/optimizations';approved='9d7b6406b2544db4c644fdacd171c4ddda3e77fbfa576fe842aa0fa3badb1bbe'
for kind in ['scope-corrected','install','managed','clients']:
 print(json.dumps({'starting_gate':kind}),flush=True)
 with (a/f'f10a-native-{kind}-wrapper.log').open('x') as out:r=subprocess.run([sys.executable,str(a/'f10a-run-native.py'),kind,approved],stdout=out,stderr=subprocess.STDOUT)
 path=a/f'f10a-native-{kind}-execution.json';record=json.loads(path.read_text()) if path.exists() else {};print(json.dumps({'gate':kind,'exit_code':r.returncode,'elapsed_seconds':record.get('elapsed_seconds'),'source_unchanged':record.get('source_unchanged')}),flush=True)
 if r.returncode:raise SystemExit(r.returncode)
