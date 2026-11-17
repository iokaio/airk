# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,subprocess,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]; BASE=Path(__file__).resolve().parent
node=ROOT/'artifacts/tools/node-v24.15.0-win-x64/node.exe'
assert hashlib.sha256(node.read_bytes()).hexdigest()=='3331e1ffe19874215472217c5e94f5a0c6d8e18c4ac7111d3937aa0ad5e9b4a5'
def snapshot(paths):
 return {p.relative_to(ROOT).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(paths) if p.is_file()}
inputs=list((ROOT/'packages/vscode/src').rglob('*'))+list((ROOT/'packages/vscode/scripts').rglob('*'))+list((ROOT/'packages/vscode').glob('tsconfig*.json'))+[ROOT/'packages/vscode/package.json',ROOT/'packages/vscode/package-lock.json',ROOT/'crates/airk-mcp/src/help/topics.rs',ROOT/'crates/airk-mcp/src/explorer_templates.json']
before=snapshot(inputs); out=snapshot((ROOT/'packages/vscode/out').rglob('*')); start=time.monotonic()
commands=[[str(node),'node_modules/typescript/bin/tsc','-p',p] for p in ('.','tsconfig.webview.json','tsconfig.assistance.json')]+[[str(node),'scripts/build-explorer.cjs']]
results=[]
with (BASE/'extension-pinned-compile.log').open('x') as log:
 for cmd in commands:
  result=subprocess.run(cmd,cwd=ROOT/'packages/vscode',stdout=log,stderr=subprocess.STDOUT)
  results.append({'command':cmd,'exit_code':result.returncode})
  if result.returncode:break
after=snapshot(inputs); output=snapshot((ROOT/'packages/vscode/out').rglob('*'))
receipt={'node':'24.15.0','commands':results,'elapsed_seconds':time.monotonic()-start,'inputs':before,'inputs_unchanged':before==after,'compiled_outputs':output,'output_bytes_unchanged':out==output}
(BASE/'extension-pinned-compile.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps({k:v for k,v in receipt.items() if k not in ('inputs','compiled_outputs')}))
raise SystemExit(next((r['exit_code'] for r in results if r['exit_code']),0) or (0 if before==after else 3))
