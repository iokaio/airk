# SPDX-License-Identifier: Apache-2.0
import subprocess,sys
pattern='a valid installation generation does not admit|replacement retires only the replaced|replacement retains retry evidence|Reconnect records interleaved|concurrent explicit and automatic proposals|Assistance probes (require explicit|preserve a wrong-server)|native fixture completion retains|native fixture job polling refuses|native fixture repair polling refuses|MCP fixture reads refuse a successful|MCP fixture reads repeat only typed rejections'
commands=[
 ('native-clippy',[sys.executable,'scripts/dev-check.py','-p','airk-cli','--features','local-inference','--target-dir','artifacts/v190/p10-target']),
 ('native-diagnostic',['cargo','test','-p','airk-cli','--no-default-features','--features','local-inference','--locked','--timings','--no-fail-fast','--lib','--test','cli_commands','replacement','--','--test-threads=1']),
 ('node-diagnostic',['node','--test','--test-concurrency=1','--test-reporter=tap','--test-name-pattern='+pattern,'packages/vscode/out/test/agentReconnect.test.js','packages/vscode/out/test/serviceOnboarding.test.js','packages/vscode/out/test/native.test.js','packages/vscode/out/test/nativeReadRetry.test.js']),
 ('python-diagnostic',[sys.executable,'-m','unittest','discover','-s','scripts/tests','-p','test_runtime_upgrade.py','-v']),
]
results={}
for name,command in commands:
 if name=='native-diagnostic' and results.get('native-clippy'):
  print('NOT RUN native assertions: collect compiler diagnostics first',flush=True);continue
 print('DIAGNOSTIC',name,command,flush=True)
 results[name]=subprocess.run(command).returncode
print('COMPLETE DIAGNOSTIC RESULTS',results,flush=True)
sys.exit(1 if any(results.values()) else 0)
