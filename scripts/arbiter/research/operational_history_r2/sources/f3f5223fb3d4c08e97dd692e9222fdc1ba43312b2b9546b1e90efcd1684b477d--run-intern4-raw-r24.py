# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,types
from pathlib import Path
root=Path.cwd();packet_path=root/'artifacts/advanced-split/models/context-action-intern4-raw-r24/execution-packet.json';packet_raw=packet_path.read_bytes();assert hashlib.sha256(packet_raw).hexdigest()=='1d2662d6988d6af21f721933e6f8dd9266e1558fdfa58e378e27ecd94c8809bb';packet=json.loads(packet_raw);work=packet_path.parent
pins=packet['sha256_pins']
def sha(p):
 with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
for name,pin in pins.items():assert sha(name)==pin,name
assert not Path(packet['environment']['AIRK_CONTEXT_ACTION_SCORE_OUTPUT']).exists();Path(packet['environment']['AIRK_CONTEXT_ACTION_PRIVATE_CONTROL']).mkdir(parents=True,exist_ok=False)
helper=root/'tools/retrieval-train/training_execution.py';raw=helper.read_bytes();assert hashlib.sha256(raw).hexdigest()=='d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975';owner=types.ModuleType('retained_context_capture_owner');owner.__file__=str(helper);exec(compile(raw,str(helper),'exec'),owner.__dict__)
env=dict(os.environ);env.update(packet['environment']);env.update(OMP_NUM_THREADS='2',MKL_NUM_THREADS='2',OPENBLAS_NUM_THREADS='2');(work/'stdin').write_bytes(b'')
with (work/'stdout.log').open('wb') as log,(work/'stdin').open('rb') as stdin:result=owner.run_windows_owned_training(packet['argv'],root,log,packet['external_owner_limits']['seconds'],env,stdin,packet['external_owner_limits']['job_committed_bytes'])
drift=[name for name,pin in pins.items() if sha(name)!=pin];receipt={'owned':result,'packet_sha256':hashlib.sha256(packet_raw).hexdigest(),'pre_post_pin_count':len(pins),'post_input_drift':drift,'optional_nn':'LargeIntern raw experiment only; no advice/task qualification','embedding_forward_calls':'Actual complete trained readout under pinned historical profile' ,'other_user_processes_touched':False};(work/'execution.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt));assert result['exit_code']==0 and not result['timed_out'] and not result['owned_job_forced_termination'] and result['owned_descendants_settled'] and not drift
assert '1 passed; 0 failed' in (work/'stdout.log').read_text()


