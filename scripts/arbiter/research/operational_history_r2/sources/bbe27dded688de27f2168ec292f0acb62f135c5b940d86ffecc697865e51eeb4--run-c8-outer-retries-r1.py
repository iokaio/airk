# SPDX-License-Identifier: Apache-2.0
import json,subprocess,time
from pathlib import Path
base=Path('artifacts/local/one-airk-1.8.14')
for name,args in [('c8-retry-adapters-compile-r1',['npm.cmd','--prefix','packages/vscode','run','compile']),('c8-retry-adapters-tests-r1',['node','--test','packages/vscode/out/test/serviceClient.test.js','packages/vscode/out/test/serviceIntents.test.js','packages/vscode/out/test/indexingCatalog.test.js','packages/vscode/out/test/indexingSplit.test.js','packages/vscode/out/test/explorerShell.test.js'])]:
 start=time.monotonic()
 with (base/(name+'.log')).open('x') as f:r=subprocess.run(args,stdout=f,stderr=subprocess.STDOUT)
 result={'arguments':args,'exit_code':r.returncode,'elapsed_seconds':time.monotonic()-start}
 with (base/(name+'.json')).open('x') as f:json.dump(result,f,indent=2)
 print(json.dumps(result),flush=True)
 if r.returncode:raise SystemExit(r.returncode)
