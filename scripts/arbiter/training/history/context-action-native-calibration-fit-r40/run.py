# SPDX-License-Identifier: Apache-2.0
"""Execute one reviewed native CAL proxy fit. No model load or activation."""
import hashlib,importlib.util,json,os,subprocess,sys,time
from pathlib import Path
root=Path(__file__).resolve().parents[4];out=Path(__file__).resolve().parent
def sha(path):
    with Path(path).open('rb') as source:return hashlib.file_digest(source,'sha256').hexdigest()
packet_path=out/'execution-packet.json'
assert len(sys.argv)==2 and sha(packet_path)==sys.argv[1],'reviewed packet SHA required'
packet=json.loads(packet_path.read_text(encoding='utf-8'))
assert packet['schema']=='airk.private.context-action-fit-execution/1'
assert packet['limits']=={'seconds':60,'job_committed_bytes':1024**3,'process_limit':2}
sid=subprocess.check_output(['whoami','/user','/fo','csv','/nh'],text=True).strip()
assert '-1012' in sid,'existing default SID1012 required'
pins=packet['sha256_pins'];before={path:sha(path) for path in pins}
assert before==pins,'pre-input drift'
assert not Path(packet['environment']['AIRK_CONTEXT_ACTION_FIT_OUTPUT']).exists(),'fresh native output required'
owner=root/'tools/retrieval-train/training_execution.py'
assert sha(owner)=='d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975'
spec=importlib.util.spec_from_file_location('native_calibration_fit_owner_r40',owner)
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
environment=os.environ.copy();environment.update(packet['environment'])
(out/'stdin').write_bytes(b'')
start=time.monotonic()
with (out/'stdout.log').open('xb') as output,(out/'stdin').open('rb') as source:
    owned=module.run_windows_owned_training(packet['argv'],str(root),output,60,environment,source,1024**3)
post={path:sha(path) for path in pins}
receipt={'schema':'airk.private.context-action-fit-execution-receipt/1','owned':owned,'argv':packet['argv'],'elapsed_seconds':time.monotonic()-start,'caller_sid':sid,'packet_sha256':sys.argv[1],'pre_post_pin_count':len(pins),'post_input_drift':[path for path in pins if post[path]!=pins[path]],'sha256_pins':pins,'optional_nn':False,'activation':False,'final_accessed':False}
(out/'execution.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
assert not receipt['post_input_drift']
assert owned['exit_code']==0 and owned['owned_descendants_settled'] and owned['owned_job_final_active_processes']==0 and not owned['owned_job_forced_termination']
log=(out/'stdout.log').read_text(encoding='utf-8')
assert 'running 1 test' in log and '1 passed; 0 failed' in log,'one actual native selection required'
result=Path(packet['environment']['AIRK_CONTEXT_ACTION_FIT_OUTPUT'])
report=json.loads((result/'fit-report.private.json').read_text(encoding='utf-8'))
assert report['complete'] and report['task_qualified'] is False and report['release_qualified'] is False and report['qualified_profiles']==[]
assert report['cal_questions']==19 and report['cal_families']==9 and len(report['models'])==4
for model in report['models']:
    assert len(model['outcomes'])==19
    if model['status']=='developer_proxy_rule_fitted':
        assert model['certified_admitted_questions']>0 and model['uncertified_admitted_questions']==0
        assert model['experimental_profile']['qualification'] is None
        assert not model['experimental_profile'].get('qualified_targets',[]) and not model['experimental_profile'].get('downloads',[])
    else:
        assert model['status']=='unsupported_no_positive_safe_margin' and model['calibration'] is None and model['experimental_profile'] is None
artifacts={str(path):sha(path) for path in result.rglob('*') if path.is_file()}
(out/'output-seal.json').write_text(json.dumps({'schema':'airk.private.context-action-fit-output-seal/1','sha256':artifacts,'task_qualified':False,'release_qualified':False,'activation':False},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'native_exit':owned['exit_code'],'selected':1,'pin_count':len(pins),'post_input_drift':receipt['post_input_drift'],'report':str(result/'fit-report.private.json'),'report_sha256':sha(result/'fit-report.private.json'),'model_statuses':[{k:m[k] for k in ['model','status']} for m in report['models']],'optional_nn':False,'activation':False}))
