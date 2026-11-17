# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,shutil,types
from pathlib import Path
root=Path.cwd();out=root/'artifacts/advanced-split/models/native-parent-bag-proposal-r1/unpublished-parity-r38';out.mkdir(exist_ok=False)
def sha(p):
 with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
source=root/'crates/airk-kernel/src/inference_owner/evaluation_tests/train_units.rs';assert sha(source)=='ae41b7bc886a85ae5fb27ad50cdb2e7b0f18bfc73724b57773e7c62218bbb5e1'
original=root/'artifacts/v190/p10-target/debug/deps/airk_kernel-57b62dce9e600d19.exe';assert sha(original)=='25f774a66e23e0992587cd725f380b959ac3a9b0263a73bd0c3e312395c93282';image=out/'airk-kernel.exe';shutil.copyfile(original,image)
master=root/'artifacts/advanced-split/models/native-encoder-qualification-input-r1.json';assert sha(master)=='3ead240726ebdba10577db6778bffbf5232eab10c50f2ae128d589e0a961ab81';row=next(r for r in json.loads(master.read_bytes()) if r['family']=='original-mixed-u8-ort130-evaluation');folder=Path(row['directory']);metadata=json.loads((folder/'model.json').read_bytes());pins={source:sha(source),image:sha(image),master:sha(master),folder/'model.json':row['model_json_sha256']}
for name,key in [('model.onnx','model_sha256'),('tokenizer.json','tokenizer_sha256'),('LICENSE','license_sha256')]:pins[folder/name]=metadata[key]
helper=root/'tools/retrieval-train/training_execution.py';raw=helper.read_bytes();assert hashlib.sha256(raw).hexdigest()=='d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975';pins[helper]=hashlib.sha256(raw).hexdigest();owner=types.ModuleType('retained_unpublished_owner');owner.__file__=str(helper);exec(compile(raw,str(helper),'exec'),owner.__dict__)
env=dict(os.environ);env.update(AIRK_NATIVE_ENCODER_INPUT=str(master),OMP_NUM_THREADS='1',MKL_NUM_THREADS='1',OPENBLAS_NUM_THREADS='1')
for suffix in ['txt','python']:
 for p,h in pins.items():assert sha(p)==h,str(p)
 selector='inference_owner::evaluation_tests::train_units::native_unpublished_'+suffix+'_matches_kernel_payload_and_planner';argv=[str(image),'--exact',selector,'--ignored','--nocapture','--test-threads=1'];(out/(suffix+'.stdin')).write_bytes(b'')
 with (out/(suffix+'.log')).open('wb') as log,(out/(suffix+'.stdin')).open('rb') as stdin:result=owner.run_windows_owned_training(argv,root,log,5,env,stdin,1024**3)
 drift=[str(p) for p,h in pins.items() if sha(p)!=h];receipt={'argv':argv,'input_pins':{str(p):h for p,h in pins.items()},'owned':result,'post_input_drift':drift,'external_limits':{'seconds':5,'bytes':1024**3,'slots':2},'embedding_forwards':0};(out/(suffix+'.json')).write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt));print((out/(suffix+'.log')).read_text())
 if result['exit_code'] or result['timed_out'] or result['owned_job_forced_termination'] or not result['owned_descendants_settled'] or drift:raise SystemExit(1)
 assert '1 passed; 0 failed' in (out/(suffix+'.log')).read_text()
