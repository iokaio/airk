# SPDX-License-Identifier: Apache-2.0
import json,subprocess,sys
node=r'C:\Program Files\nodejs\node.exe'
codes=[]
for config in ['.','tsconfig.webview.json','tsconfig.assistance.json']:
 cmd=[node,'node_modules/typescript/bin/tsc','-p',config]
 print(json.dumps({'compile_command':cmd,'cwd':'packages/vscode'}),flush=True)
 codes.append(subprocess.run(cmd,cwd='packages/vscode').returncode)
print(json.dumps({'configuration_exit_codes':codes}),flush=True)
if any(codes):sys.exit(1)
sys.exit(subprocess.run([node,'scripts/build-explorer.cjs'],cwd='packages/vscode').returncode)