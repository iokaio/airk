# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import subprocess,sys,json,time
root=Path.cwd();a=root/'artifacts/optimizations';approved='e4f99021403f673e689930fbb9edcec9829924dcab33c0badc7c5799d9fca2a6'
for kind in ['pool-corrected','semantic','scope','smoke','assistance','install','managed','console','console-client','clients']:
 print(json.dumps({'starting_gate':kind}),flush=True)
 with (a/f'f12a-native-{kind}-wrapper.log').open('x',encoding='utf-8') as out:
  result=subprocess.run([sys.executable,str(a/'f12a-run-native.py'),kind,approved],stdout=out,stderr=subprocess.STDOUT)
 receipt=a/f'f12a-native-{kind}-execution.json';record=json.loads(receipt.read_text()) if receipt.exists() else {}
 print(json.dumps({'gate':kind,'exit_code':result.returncode,'elapsed_seconds':record.get('elapsed_seconds'),'source_unchanged':record.get('source_unchanged')}),flush=True)
 if result.returncode:raise SystemExit(result.returncode)
