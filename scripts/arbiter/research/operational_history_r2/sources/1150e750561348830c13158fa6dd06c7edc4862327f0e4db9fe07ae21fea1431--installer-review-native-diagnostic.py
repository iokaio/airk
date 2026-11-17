# SPDX-License-Identifier: Apache-2.0
import subprocess,sys
commands=[
 ('native-clippy',[sys.executable,'scripts/dev-check.py','-p','airk-cli','--features','local-inference','--target-dir','artifacts/v190/p10-target']),
 ('native-diagnostic',['cargo','test','-p','airk-cli','--no-default-features','--features','local-inference','--locked','--timings','--no-fail-fast','--lib','--test','cli_commands','replacement','--','--test-threads=1']),
 ('trust-diagnostic',['node','--test','--test-reporter=tap','--test-name-pattern=replacement retains retry evidence','packages/vscode/out/test/agentReconnect.test.js']),
 ('python-correction',[sys.executable,'-m','unittest','discover','-s','scripts/tests','-p','test_runtime_upgrade.py','-v']),
]
results={}
for name,command in commands:
 if name=='native-diagnostic' and results.get('native-clippy'):
  print('NOT RUN native assertions: collect compiler diagnostics first',flush=True);continue
 print('DIAGNOSTIC',name,command,flush=True)
 results[name]=subprocess.run(command).returncode
print('COMPLETE DIAGNOSTIC RESULTS',results,flush=True)
sys.exit(1 if any(results.values()) else 0)
