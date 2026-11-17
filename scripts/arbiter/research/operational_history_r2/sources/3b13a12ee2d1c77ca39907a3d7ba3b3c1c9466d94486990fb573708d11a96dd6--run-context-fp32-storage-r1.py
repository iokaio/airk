# SPDX-License-Identifier: Apache-2.0
import hashlib,json,subprocess,sys
from pathlib import Path
r=Path.cwd();p=r/'artifacts/advanced-split/models/qwen3-challengers-r1/context-fp32-storage-r1'
python=str(r/'artifacts/advanced-split/models/training-env/venv/Scripts/python.exe')
for label in ['17','4']:
 d=json.loads((p/f'release-template-{label}-fp32-storage.json').read_bytes())
 d['execution_authorized']=True
 d['root_review']='Root self-review of pack_onnx/prepare_storage/storage_worker and seven independent controls. Only external tensor byte relocation with full protobuf restoration and tensor range hashes; no neural execution, weights or precision change. Existing caps preserved. Native/combined/resource/quality gates remain pending.'
 release=p/f'release-{label}-fp32-storage.json'
 assert not release.exists();release.write_text(json.dumps(d,indent=2)+'\n',encoding='utf-8')
 pin=hashlib.sha256(release.read_bytes()).hexdigest()
 prepared=subprocess.run([python,'-B',str(p/'prepare_storage.py'),'--release',str(release),'--release-sha256',pin],capture_output=True,text=True)
 (p/f'prepare-{label}-root.json').write_text(json.dumps({'exit_code':prepared.returncode,'stdout':prepared.stdout,'stderr':prepared.stderr},indent=2)+'\n')
 if prepared.returncode:print(prepared.stderr,flush=True);raise SystemExit(prepared.returncode)
 packet=json.loads(prepared.stdout);print(json.dumps({'label':label,'prepared':packet}),flush=True)
 with (p/f'root-launch-{label}.log').open('x') as log:result=subprocess.run(packet['launch_command'],stdout=log,stderr=subprocess.STDOUT)
 print(json.dumps({'label':label,'launch_exit_code':result.returncode}),flush=True)
 if result.returncode:raise SystemExit(result.returncode)
