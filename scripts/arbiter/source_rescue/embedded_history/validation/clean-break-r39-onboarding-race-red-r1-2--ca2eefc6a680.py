# SPDX-License-Identifier: Apache-2.0
import subprocess,sys
steps=[[r'C:\Program Files\nodejs\node.exe','packages/vscode/node_modules/typescript/bin/tsc','-p','packages/vscode'],[r'C:\Program Files\nodejs\node.exe','--test','--test-name-pattern=live native owner replacement refreshes owned HTTP port after 2 transient failures','packages/vscode/out/test/serviceOnboarding.test.js']]
for step in steps:
 result=subprocess.run(step)
 if result.returncode:sys.exit(result.returncode)