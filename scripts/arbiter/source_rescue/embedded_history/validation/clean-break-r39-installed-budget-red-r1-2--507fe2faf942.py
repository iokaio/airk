# SPDX-License-Identifier: Apache-2.0
import subprocess,sys
node=r'C:\Program Files\nodejs\node.exe'
r=subprocess.run([node,'node_modules/typescript/bin/tsc','-p','.'],cwd='packages/vscode')
if r.returncode:sys.exit(r.returncode)
sys.exit(subprocess.run([node,'--test','packages/vscode/out/test/installedEditorContracts.test.js']).returncode)