# SPDX-License-Identifier: Apache-2.0
import hashlib,json,subprocess
from pathlib import Path
r=Path.cwd();p=r/'artifacts/advanced-split/models/qwen3-challengers-r1/context-fp32-readout-r1'
python=str(r/'artifacts/advanced-split/models/training-env/venv/Scripts/python.exe')
for label in ['17','4']:
 d=json.loads((p/f'release-template-{label}-fp32-readout.json').read_bytes());d['execution_authorized']=True
 d['root_review']='Root reviewed derive/prepare/worker sources and three cheap controls. Actual CPU trained-row readout on pinned original official hidden vectors only, no backbone or ORT execution. All-live H and original known-live same-artifact controls retain strict 0.001 tolerance; no task/gold or calibration. Separately scheduled NN window required for launch; fixed resource/process limits preserved.'
 release=p/f'release-{label}-fp32-readout.json';assert not release.exists();release.write_text(json.dumps(d,indent=2)+'\n',encoding='utf-8')
 pin=hashlib.sha256(release.read_bytes()).hexdigest()
 result=subprocess.run([python,'-B',str(p/'prepare_readout.py'),'--release',str(release),'--release-sha256',pin],capture_output=True,text=True)
 packet={'exit_code':result.returncode,'stdout':result.stdout,'stderr':result.stderr}
 (p/f'root-preparation-{label}.json').write_text(json.dumps(packet,indent=2)+'\n')
 if result.returncode:print(result.stderr,flush=True);raise SystemExit(result.returncode)
 command=json.loads(result.stdout);(p/f'root-launch-command-{label}.json').write_text(json.dumps(command,indent=2)+'\n')
 print(json.dumps(command),flush=True)
