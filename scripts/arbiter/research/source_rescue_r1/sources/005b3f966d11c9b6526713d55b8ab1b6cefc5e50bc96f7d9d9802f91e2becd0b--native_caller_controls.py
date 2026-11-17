# SPDX-License-Identifier: Apache-2.0
"""PRIVATE unchanged native-owner caller integration controls; no AIRK/NN."""
import json
import os
from pathlib import Path
import sys
import time
import runner

ROOT=Path(__file__).parent
owner,parent,worker=runner.reviewed_reference()
output=ROOT/'native-caller-controls';output.mkdir()
rows=[]
for name in ['natural_single_child','sid_refusal_before_resume','image_refusal_before_resume']:
    marker=output/(name+'.marker');log=output/(name+'.log')
    started=time.monotonic();deadline=started+5
    expected={'path':str(Path(sys.executable).resolve()),'sha256':runner.digest(sys.executable)}
    human=runner.SID
    if name=='sid_refusal_before_resume':human='deliberately-invalid-expected-sid'
    if name=='image_refusal_before_resume':expected['sha256']='0'*64
    def auth(api,job,info,child_deadline):
        return parent.suspended_authentication(api,info,deadline,child_deadline,human,expected,owner.image_proof,sid_reader=worker.windows_sid)
    error=None;actual=None
    with log.open('xb') as stream,open(os.devnull,'rb') as null_input:
        try:
            actual=owner.run_windows_owned_training([sys.executable,'-B','-S',str(ROOT/'native_owner_probe.py'),str(marker)],ROOT,stream,deadline-time.monotonic(),dict(os.environ,PYTHONDONTWRITEBYTECODE='1'),null_input,8<<30,before_resume=auth,absolute_deadline=deadline)
        except ValueError as failure:error=failure
    if name=='natural_single_child':
        assert error is None and marker.exists() and parent.settled(actual),actual
        body=json.loads(log.read_text(encoding='utf-8-sig').strip())
        assert body['actual_sid']==runner.SID and body['actual_job']['job_memory']==8<<30
        rows.append({'name':name,'passed':True,'actual_owner_result':actual,'actual_child':body})
    else:
        expected_message=('suspended child identity differs from verified human parent' if name.startswith('sid_') else 'owned interpreter image differs from pinned runtime')
        assert error is not None and str(error)==expected_message and not marker.exists(),str(error)
        cleanup=getattr(error,'owned_cleanup',None)
        assert cleanup is not None and cleanup['settled'] is True and cleanup['final_member_pids']==[] and cleanup['final_active']==0,cleanup
        rows.append({'name':name,'passed':True,'actual_refusal':{'type':type(error).__name__,'message':str(error)},'no_resume_marker':True,'actual_cleanup':cleanup})
receipt={'schema':'airk.private.unchanged-native-owner-caller-controls/1','selected':3,'passed':3,'failed':0,'owner_sha256':runner.OWNER_SHA,'parent_guard_sha256':runner.PARENT_SHA,'actual_service_or_nn':False,'cases':rows}
with (output/'result.json').open('x',encoding='utf-8') as stream:json.dump(receipt,stream,indent=2);stream.write('\n')
print(json.dumps({'selected':3,'passed':3,'failed':0,'receipt':str(output/'result.json'),'airk_or_nn_executed':False}))
