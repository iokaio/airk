# SPDX-License-Identifier: Apache-2.0
"""Pure independent refusal controls; no native/model work."""
from pathlib import Path
import copy
import json
import types
HERE=Path(__file__).resolve().parent
source=HERE/'run_stage.py'
owner=types.ModuleType('_pure_matched_dev');owner.__file__=str(source)
exec(compile(source.read_bytes(),str(source),'exec'),owner.__dict__)
ids=['independent-q'+str(i) for i in range(64)]
packet={'capture_common':{'query':{'mode':'vector','limit':10,'max_bytes':32768,'full_chunks':True},'quality_denominator':20,'alignment_refusals':44},'source_roster_sha256':'source-seal','original_question_order':ids,'source_root':str(HERE)}
stage={'family':'test-family','model_json_sha256':'model','descriptor_sha256':'descriptor'}
valid={'complete':True,'family':'test-family',**packet['capture_common'],'model_json_sha256':'model','encoder_input_sha256':'descriptor','source_before_sha256':'source-seal','source_after_sha256':'source-seal','collections':[{'reused':False,'publication':'pub'}],'records':[{'id':i,'query_us':0,'source_root':str(HERE),'primary_vector':{'publication':'pub','hits':[]}} for i in ids]}
owner.validate_capture(valid,stage,packet)
controls=1
def refusal(change):
 global controls
 value=copy.deepcopy(valid);change(value)
 try:owner.validate_capture(value,stage,packet)
 except (ValueError,KeyError,TypeError,OSError):controls+=1;return
 raise AssertionError('independent invalid native capture accepted')
refusal(lambda v:v.update(complete=False))
refusal(lambda v:v.update(family='foreign'))
refusal(lambda v:v.update(model_json_sha256='foreign'))
refusal(lambda v:v.update(encoder_input_sha256='foreign'))
refusal(lambda v:v['records'].reverse())
refusal(lambda v:v['records'].pop())
refusal(lambda v:v['collections'][0].update(reused=True))
refusal(lambda v:v['records'][0].update(native_error='actual-error'))
refusal(lambda v:v['records'][0]['primary_vector'].update(publication='foreign'))
refusal(lambda v:v['records'][0]['primary_vector'].update(hits=[{}]*11))
refusal(lambda v:v.update(source_after_sha256='changed'))
refusal(lambda v:v.update(quality_denominator=64))
assert owner.isolated(HERE/'future-output',HERE/'frozen-input')
assert not owner.isolated(HERE,HERE/'nested-input')
assert not owner.isolated(HERE/'nested-output',HERE)
assert not owner.isolated(HERE,HERE)
controls+=4
natural={'exit_code':0,'timed_out':False,'log_oversized':False,'owned_descendants_settled':True,'owned_job_forced_termination':False,'owned_job_final_active_processes':0,'owned_job_final_member_pids':[],'owned_total_processes':1}
assert owner.natural(natural)
assert not owner.natural({**natural,'owned_job_forced_termination':True})
assert not owner.natural({**natural,'owned_total_processes':2})
controls+=3
print(json.dumps({'pure_controls_passed':controls,'native_execution':False,'model_execution':False}))
