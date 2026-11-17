# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,types
from pathlib import Path
root=Path.cwd();packet_path=root/'artifacts/advanced-split/models/context-action-native-capture-r23/execution-packet.json';packet_raw=packet_path.read_bytes();assert hashlib.sha256(packet_raw).hexdigest()=='f757d46256fe1f6d2c0c97b11332ec5ad5fc36674a48a8ddaaba3ba58be6634f';packet=json.loads(packet_raw);work=packet_path.parent
pins={**packet['sha256_pins'],**packet['source_file_sha256']}
def sha(p):
 with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
for name,pin in pins.items():assert sha(name)==pin,name
assert not Path(packet['environment']['AIRK_CONTEXT_ACTION_CAPTURE_OUTPUT']).exists();Path(packet['environment']['AIRK_CONTEXT_ACTION_PRIVATE_CONTROL']).mkdir(parents=True,exist_ok=False)
helper=root/'tools/retrieval-train/training_execution.py';raw=helper.read_bytes();assert hashlib.sha256(raw).hexdigest()=='d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975';owner=types.ModuleType('retained_context_capture_owner');owner.__file__=str(helper);exec(compile(raw,str(helper),'exec'),owner.__dict__)
env=dict(os.environ);env.update(packet['environment']);env.update(OMP_NUM_THREADS='2',MKL_NUM_THREADS='2',OPENBLAS_NUM_THREADS='2');(work/'stdin').write_bytes(b'')
with (work/'stdout.log').open('wb') as log,(work/'stdin').open('rb') as stdin:result=owner.run_windows_owned_training(packet['argv'],root,log,1800,env,stdin,8*1024**3)
drift=[name for name,pin in pins.items() if sha(name)!=pin];receipt={'owned':result,'packet_sha256':hashlib.sha256(packet_raw).hexdigest(),'pre_post_pin_count':len(pins),'post_input_drift':drift,'optional_nn':False,'embedding_forward_calls':'actual default MiniLM indexing/read path; not optional Arbiter scoring','other_user_processes_touched':False};(work/'execution.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt));assert result['exit_code']==0 and not result['timed_out'] and not result['owned_job_forced_termination'] and result['owned_descendants_settled'] and not drift
assert '1 passed; 0 failed' in (work/'stdout.log').read_text()
