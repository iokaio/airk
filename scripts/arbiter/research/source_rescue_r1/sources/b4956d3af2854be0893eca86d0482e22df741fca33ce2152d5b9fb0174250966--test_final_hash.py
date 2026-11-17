# SPDX-License-Identifier: Apache-2.0
"""PRIVATE final stdout hashing / one-final-clock regressions; zero child/service/NN."""
import copy
import hashlib
import json
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).parents[1]))
import runner

ROOT=Path(__file__).parent
log=ROOT/'literal-stdout.log';log.write_bytes(b'actual retained stdout\n')
expected_sha=hashlib.sha256(b'actual retained stdout\n').hexdigest()
rows=[]
for name,end_time,prior_error in [
    ('hash_crosses_original_cutoff',100.25,None),
    ('hash_finishes_before_original_cutoff',99.0,None),
    ('hash_late_preserves_original_failure',100.25,{'type':'ValueError','message':'actual native refusal'}),
    ('hash_exactly_at_original_cutoff_refuses',100.0,None),
]:
    state={'now':98.0,'clock_calls':0,'order':[]}
    receipt={'complete':prior_error is None,'failure':copy.deepcopy(prior_error)}
    def hash_then_advance(path):
        state['order'].append('hash')
        result=runner.digest(path)
        state['now']=end_time
        return result
    def one_clock():
        state['order'].append('clock')
        state['clock_calls']+=1
        return state['now']
    late=runner.final_phase_observation(receipt,log,90.0,100.0,clock=one_clock,hash_file=hash_then_advance)
    assert state['order']==['hash','clock'] and state['clock_calls']==1,state
    assert receipt['stdout']['sha256']==expected_sha
    assert receipt['elapsed_seconds']==end_time-90.0
    assert receipt['original_phase_expired']==(end_time>=100.0)
    if prior_error is not None:
        assert receipt['failure']==prior_error and receipt['complete'] is False and late is False
    elif end_time>=100.0:
        assert receipt['complete'] is False and receipt['failure']['type']=='TimeoutError' and late is True
    else:
        assert receipt['complete'] is True and receipt['failure'] is None and late is False
    rows.append({'name':name,'passed':True,'callback_order':state['order'],'clock_calls':state['clock_calls'],'actual_final_observation':receipt,'late_failure_requires_nonzero_exit':late})
result={'schema':'airk.private.final-hash-cutoff-controls/1','selected':4,'passed':4,'failed':0,'actual_children_services_or_nn':False,'cases':rows}
with (ROOT/'result.json').open('x',encoding='utf-8') as stream:json.dump(result,stream,indent=2);stream.write('\n')
print(json.dumps({'selected':4,'passed':4,'failed':0,'receipt':str(ROOT/'result.json'),'service_or_nn':False}))
