# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[2]
commands=[]
for mode in ('off','report'):
 commands.append((f'control-{mode}', ['python','scripts/benchmark-mcp-investigation.py','owned','--runtime-dir','artifacts/v190/p6a-p5-control-copy','--output',f'artifacts/v190/p6f-final-control-{mode}-mcp','--label','baseline','--samples','30','--concurrency','1','--verification',mode]))
for mode in ('off','report'):
 commands.append((f'candidate-{mode}', ['python','artifacts/v190/p6f-resume-frozen-upgrade-replay-r5.py','--verification',mode,'--output',f'artifacts/v190/p6f-final-{mode}-mcp']))
results=[]
for name,command in commands:
 print('Measuring frozen workload: '+name,flush=True);started=time.monotonic()
 with (ROOT/f'artifacts/v190/p6f-final-{name}.log').open('w',encoding='utf-8') as log:
  result=subprocess.run(command,cwd=ROOT,stdout=log,stderr=subprocess.STDOUT,creationflags=0x08000000)
 results.append({'name':name,'returncode':result.returncode,'elapsed_seconds':time.monotonic()-started})
 (ROOT/'artifacts/v190/p6f-final-performance-campaign.json').write_text(json.dumps(results,indent=2))
 print(json.dumps(results[-1]),flush=True)
if any(row['returncode'] for row in results):raise SystemExit(1)
result=subprocess.run(['python','artifacts/v190/p6f-resume-evaluate-gates.py'],cwd=ROOT,creationflags=0x08000000)
raise SystemExit(result.returncode)
