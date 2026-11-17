# SPDX-License-Identifier: Apache-2.0
"""PRIVATE five real Windows custody controls; one batch, no AIRK/NN/Cargo."""
import importlib.util
import json
import os
from pathlib import Path
import sys
import time

ROOT=Path(__file__).parent
spec=importlib.util.spec_from_file_location('private_transport_owner',ROOT/'windows_job_8g.py')
owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)

def need(value,message):
    if not value:raise AssertionError(message)

def natural(result,expected_exit):
    return (result['exit_code']==expected_exit and result['timed_out'] is False and result['log_oversized'] is False and result['owned_descendants_settled'] is True and result['owned_job_forced_termination'] is False and result['owned_job_final_active_processes']==0 and result['owned_job_final_member_pids']==[] and result['child_assigned_before_resume'] is True and result['parent_assigned_to_job'] is False)

def run():
    evidence=ROOT/'controls'
    evidence.mkdir()
    environment=dict(os.environ,PYTHONDONTWRITEBYTECODE='1')
    cases=[]
    for name,scenario,expected in [('three_children_acceptance','accept',0),('three_children_native_memory_refusal','memory',0),('three_children_nonzero_refusal','nonzero',23)]:
        log=evidence/(name+'.log')
        began=time.monotonic();deadline=began+15
        with log.open('xb') as output:
            result=owner.run_windows_owned([sys.executable,'-B','-S',str(ROOT/'probe.py'),'leader',scenario],ROOT,output,15,environment,absolute_deadline=deadline)
        lines=[json.loads(line) for line in log.read_text(encoding='utf-8-sig').splitlines() if line.startswith('{')]
        need(natural(result,expected),'Actual natural child/job settlement or original exit class differed: '+str(result))
        ready=next(row for row in lines if row.get('all_three_child_handles_live'))
        need(len(ready['children'])==3,'Original three-child denominator lost')
        if scenario=='memory':need(ready['children'][0]['native_memory_refusal']['allocation_returned_null'] is True,'Actual native8GiB admission refusal missing')
        cases.append({'name':name,'passed':True,'elapsed_seconds':time.monotonic()-began,'actual_owner_result':result,'actual_child_observations':lines})
    marker=evidence/'expired-resume-marker.txt';log=evidence/'expired_before_resume.log'
    expired=False
    with log.open('xb') as output:
        try:owner.run_windows_owned([sys.executable,'-B','-S',str(ROOT/'probe.py'),'marker',str(marker)],ROOT,output,6,environment,absolute_deadline=time.monotonic()-1)
        except TimeoutError as error:expired=str(error)=='Original job phase expired before child resume'
    need(expired and not marker.exists() and log.stat().st_size==0,'Expired original phase actually resumed a child')
    cases.append({'name':'expired_original_deadline_refuses_before_resume','passed':True,'marker_absent':True,'child_log_bytes':0})
    log=evidence/'absolute_clip.log';began=time.monotonic();deadline=began+1
    with log.open('xb') as output:
        result=owner.run_windows_owned([sys.executable,'-B','-S',str(ROOT/'probe.py'),'wait'],ROOT,output,6,environment,absolute_deadline=deadline)
    elapsed=time.monotonic()-began
    need('waiting_on_unsignaled_event' in log.read_text(encoding='utf-8-sig'),'Clip case never actually resumed')
    need(result['timed_out'] is True and result['owned_job_forced_termination'] is True and elapsed<6,'Original absolute deadline did not clip longer relative timeout')
    need(not natural(result,0),'Forced timeout became a successful natural completion')
    cases.append({'name':'absolute_original_deadline_clips_longer_relative_timeout','passed':True,'elapsed_seconds':elapsed,'relative_seconds':6,'original_absolute_seconds':1,'actual_owner_result':result})
    receipt={'schema':'airk.private.transport-owner-controls/1','selected':5,'passed':len(cases),'failed':0,'actual_service_or_nn':False,'cases':cases}
    with (evidence/'result.json').open('x',encoding='utf-8') as stream:json.dump(receipt,stream,indent=2);stream.write('\n')
    print(json.dumps({'selected':5,'passed':len(cases),'failed':0,'receipt':str(evidence/'result.json'),'airk_or_nn_executed':False}))

if __name__=='__main__':run()
