# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,types
from pathlib import Path
root=Path.cwd();work=root/'artifacts/advanced-split/models/context-action-native-preparation-r23';work.mkdir(exist_ok=False);facts=root/'artifacts/advanced-split/models/context-action-publisher-facts-r1/publisher-facts.private.json';binary=root/'artifacts/advanced-split/native-harnesses/context-r23/airk-kernel.exe'
def sha(path):
 with path.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
assert sha(facts)=='115f9baacef8d9cac1c9602447a854a11787ed759e2f61897eb01009ae6d4254';assert sha(binary)=='d8a3a969e8b0024bb96407538aebc2176fe7fffe4ada79db3a94343aeede98cf'
path=root/'tools/retrieval-train/training_execution.py';raw=path.read_bytes();assert hashlib.sha256(raw).hexdigest()=='d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975';owner=types.ModuleType('retained_context_prepare_owner');owner.__file__=str(path);exec(compile(raw,str(path),'exec'),owner.__dict__)
env=dict(os.environ);env.update(AIRK_CONTEXT_ACTION_FACTS=str(facts),AIRK_CONTEXT_ACTION_PREPARED=str(work/'prepared'))
argv=[str(binary),'--exact','inference_owner::context_action_tests::prepare_context_action_publisher_sources','--ignored','--nocapture','--test-threads=1'];(work/'stdin').write_bytes(b'')
with (work/'stdout.log').open('wb') as log,(work/'stdin').open('rb') as stdin:result=owner.run_windows_owned_training(argv,root,log,60,env,stdin,1024**3)
assert sha(facts)=='115f9baacef8d9cac1c9602447a854a11787ed759e2f61897eb01009ae6d4254' and sha(binary)=='d8a3a969e8b0024bb96407538aebc2176fe7fffe4ada79db3a94343aeede98cf'
(work/'execution.json').write_text(json.dumps({'owned':result,'binary_sha256':sha(binary),'facts_sha256':sha(facts),'prepared_input':str(work/'prepared/capture-input.json')},indent=2)+'\n');assert result['exit_code']==0 and not result['timed_out'] and not result['owned_job_forced_termination'] and result['owned_descendants_settled'];assert '1 passed; 0 failed' in (work/'stdout.log').read_text()
identity=json.loads((work/'prepared/capture-input.identity.json').read_bytes());print(json.dumps({'passed':1,'owned':result,'capture_input_identity':identity}))
