# SPDX-License-Identifier: Apache-2.0
import subprocess,sys,time,json
from pathlib import Path
root=Path(__file__).resolve().parents[2];node=root/'artifacts/tools/node-v24.15.0-win-x64/node.exe'
commands=[
 [str(node),'packages/vscode/node_modules/typescript/bin/tsc','-p','packages/vscode/tsconfig.json'],
 [str(node),'packages/vscode/scripts/build-explorer.cjs','--write-mcp-catalog'],
 [str(node.parent/'npm.cmd'),'--prefix','packages/vscode','run','compile'],
 [sys.executable,'scripts/release-version.py','check','--expected','1.8.23'],
]
start=time.monotonic()
with (root/'artifacts/one-process/installer-1823-prepare.log').open('x') as log:
 for command in commands:
  print('COMMAND',command,file=log,flush=True)
  result=subprocess.run(command,cwd=root,stdout=log,stderr=subprocess.STDOUT)
  if result.returncode:sys.exit(result.returncode)
print(json.dumps({'version':'1.8.23','prepare_compile':'passed','seconds':time.monotonic()-start}))
