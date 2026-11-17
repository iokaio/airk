# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,types
from pathlib import Path
root=Path.cwd();out=root/'artifacts/advanced-split/models/candidate-native-emission-r20';out.mkdir(exist_ok=False)
helper=root/'scripts/provision-static-onnx-runtime.py';raw=helper.read_bytes();assert hashlib.sha256(raw).hexdigest()=='bdbdf773716af7184da59ee17b9ec718f657874cfe15efea8462fbbae27fc59a'
owner=types.ModuleType('retained_native_emission_owner');owner.__file__=str(helper);exec(compile(raw,str(helper),'exec'),owner.__dict__)
binary=root/'artifacts/advanced-split/native-harnesses/context-r20/airk-kernel.exe'
with binary.open('rb') as f:assert hashlib.file_digest(f,'sha256').hexdigest()=='0f6ddd264fa8c0ed41c6c2af545b86cf0cf138fb7fc583ccb964784837ed2fd2'
values={'AIRK_CANDIDATE_EXPORTS':root/'artifacts/advanced-split/models/visible-prefix-mining-exports-r1','AIRK_CANDIDATE_ORACLE':root/'artifacts/advanced-split/models/visible-prefix-mining-native-inputs-r1/receipt.json','AIRK_CANDIDATE_STAGE_SEAL':root/'artifacts/advanced-split/models/visible-prefix-export-execution-r1/seal.json','AIRK_CANDIDATE_MASTER_OUTPUT':out/'master.json'}
os.environ.update({k:str(v) for k,v in values.items()})
argv=[str(binary),'--exact','inference_owner::evaluation_tests::candidate::emit_selected_epoch1_candidate_native_manifests','--ignored','--nocapture','--test-threads=1']
result=owner.run_owned(argv,root,out/'stdout.log',60)
(out/'execution.json').write_text(json.dumps({'owned':result,'environment':{k:str(v) for k,v in values.items()},'binary_sha256':'0f6ddd264fa8c0ed41c6c2af545b86cf0cf138fb7fc583ccb964784837ed2fd2'},indent=2)+'\n')
assert owner.successful(result) and result['owned_job_forced_termination'] is False,result
master=out/'master.json'
with master.open('rb') as f:pin=hashlib.file_digest(f,'sha256').hexdigest()
print(json.dumps({'master':str(master),'master_sha256':pin,'owned':result}))
