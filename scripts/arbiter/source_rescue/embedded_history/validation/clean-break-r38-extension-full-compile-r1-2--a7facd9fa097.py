# SPDX-License-Identifier: Apache-2.0
import json,subprocess,sys
node=r'C:\Program Files\nodejs\node.exe'
commands=[[node,'node_modules/typescript/bin/tsc','-p',value] for value in ('.','tsconfig.webview.json','tsconfig.assistance.json')]
results=[]
for command in commands:
    print(json.dumps({'compile_command':command,'cwd':'packages/vscode'}),flush=True)
    results.append(subprocess.run(command,cwd='packages/vscode').returncode)
if any(results):
    print(json.dumps({'configuration_exit_codes':results,'bundle':'not run after compilation failure'}),flush=True)
    sys.exit(1)
command=[node,'scripts/build-explorer.cjs']
print(json.dumps({'compile_command':command,'cwd':'packages/vscode'}),flush=True)
sys.exit(subprocess.run(command,cwd='packages/vscode').returncode)