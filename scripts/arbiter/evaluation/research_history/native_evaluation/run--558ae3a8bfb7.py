# SPDX-License-Identifier: Apache-2.0
import hashlib, importlib.util, json, os, time
from pathlib import Path
root=Path(__file__).resolve().parents[4]
out=Path(__file__).resolve().parent
def sha(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
owner=root/'tools/retrieval-train/training_execution.py'
assert sha(owner)=='d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975'
binary=root/'artifacts/advanced-split/native-harnesses/context-empty-r24/airk-kernel.exe'
capture=root/'artifacts/advanced-split/models/context-action-native-capture-r23/native-capture.private.json'
facts=root/'artifacts/advanced-split/models/context-action-publisher-facts-r1/publisher-facts.private.json'
pins={str(owner):sha(owner),str(binary):'1897207aeb9e276ddb477228042d7c890824acc915b2df5da49a2264ac1890f7',str(capture):'725f3ec7950c7371cc3663d0d3c1cb3475d4674f1104d994002274b7bf3e1105',str(facts):'115f9baacef8d9cac1c9602447a854a11787ed759e2f61897eb01009ae6d4254'}
packet=json.loads((root/'artifacts/advanced-split/models/context-action-native-capture-r23/execution-packet.json').read_text())
pins.update(packet['source_file_sha256'])
for path,want in pins.items():assert sha(path)==want,(path,'pre-input drift')
spec=importlib.util.spec_from_file_location('owned_context_preflight_empty_r24',owner)
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
argv=[str(binary),'--exact','inference_owner::context_action_tests::preflight_context_action_publisher_capture_and_reference_spans','--ignored','--nocapture','--test-threads=1']
env=os.environ.copy();env.update({'AIRK_CONTEXT_ACTION_FACTS':str(facts),'AIRK_CONTEXT_ACTION_CAPTURE':str(capture),'AIRK_CONTEXT_ACTION_CAPTURE_SHA256':pins[str(capture)],'AIRK_CONTEXT_ACTION_PREFLIGHT_OUTPUT':str(out/'reference-preflight.private.json')})
(out/'stdin').write_bytes(b'')
started=time.monotonic()
with (out/'stdout.log').open('xb') as log,(out/'stdin').open('rb') as source:
    result=module.run_windows_owned_training(argv,str(root),log,60,env,source,1024**3)
post={path:sha(path) for path in pins}
receipt={'schema':'airk.private.context-action-preflight-execution/1','argv':argv,'owned':result,'elapsed_seconds':time.monotonic()-started,'sha256_pins':pins,'post_input_drift':[path for path in pins if pins[path]!=post[path]],'optional_nn':False,'limits':{'seconds':60,'bytes':1024**3,'processes':2}}
(out/'execution.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
print(json.dumps({k:v for k,v in receipt.items() if k != "sha256_pins"},indent=2))
assert not receipt['post_input_drift']
assert result['exit_code']==0 and result['owned_descendants_settled'] and not result['owned_job_forced_termination'] and result['owned_job_final_active_processes']==0
