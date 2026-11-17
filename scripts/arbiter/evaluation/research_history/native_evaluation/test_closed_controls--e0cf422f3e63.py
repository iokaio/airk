# SPDX-License-Identifier: Apache-2.0
"""Pure closed FINAL-admission controls. Synthetic only, no sealed inputs or native calls."""
from pathlib import Path
import copy,json,types
HERE=Path(__file__).resolve().parent

def module(name):
 p=HERE/(name+'.py');m=types.ModuleType('_pure_'+name);m.__file__=str(p);exec(compile(p.read_bytes(),str(p),'exec'),m.__dict__);return m
n=module('run_final');f=module('finalize_reviewed');compose=module('compose_native_descriptors')
p={'candidate_freeze_sha256':'literal-freeze','capture_common':{'runtime':'literal-runtime','query':{'mode':'vector','limit':10,'max_bytes':32768,'full_chunks':True}}}
stage={'family':'literal-candidate'};freeze={'families':[{'family':'literal-candidate','model_json_sha256':'literal-model','pipeline_digest':'literal-pipeline','encoder_input_sha256':'literal-descriptors','runtime':'literal-runtime'}]}
capture={'schema':'airk.native-fresh-final-retrieval-family/1','complete':True,'family':'literal-candidate','candidate_freeze_sha256':'literal-freeze',**p['capture_common'],'model_json_sha256':'literal-model','pipeline_digest':'literal-pipeline','encoder_input_sha256':'literal-descriptors','source_before_sha256':'source','source_after_sha256':'source','source_roster_sha256':'source','collections':[{'publication':'pub','reused':False}],'records':[{'id':'synthetic-'+str(i),'primary_vector':{'publication':'pub','hits':[]}} for i in range(205)]}
n.validate_capture(capture,stage,p,freeze);passed=1
changes=[lambda c:c.update(complete=False),lambda c:c.update(candidate_freeze_sha256='foreign'),lambda c:c.update(runtime='foreign'),lambda c:c.update(pipeline_digest='foreign'),lambda c:c.update(source_after_sha256='changed'),lambda c:c['records'].pop(),lambda c:c['records'][0].update(id='synthetic-1'),lambda c:c['collections'][0].update(reused=True),lambda c:c['records'][0]['primary_vector'].update(publication='foreign'),lambda c:c['records'][0]['primary_vector'].update(hits=[{}]*11)]
for change in changes:
 c=copy.deepcopy(capture);change(c)
 try:n.validate_capture(c,stage,p,freeze)
 except (ValueError,KeyError,TypeError):passed+=1
 else:raise AssertionError('invalid synthetic FINAL output admitted')
decision={'schema':'airk.private-final-calibration-scope-record/1','status':'RECORDED_BEFORE_ANY_FRESH_FINAL_MODEL_OUTCOME','scope':'embedding_dynamic_quantization_export_provenance_only','proposal_sha256':'synthetic-proposal','decision':'SYNTHETIC TEST ONLY','positive_learned_CAL_fit_claimed':False,'optional_Arbiter_CAL_remains_unsupported':True}
f.validate_scope_record(decision,'synthetic-proposal');passed+=1
for key,val in [('status','PROPOSED'),('scope','optional_models_are_qualified'),('proposal_sha256','foreign'),('decision',''),('positive_learned_CAL_fit_claimed',True),('optional_Arbiter_CAL_remains_unsupported',False)]:
 d=copy.deepcopy(decision);d[key]=val
 try:f.validate_scope_record(d,'synthetic-proposal')
 except (ValueError,KeyError,TypeError):passed+=1
 else:raise AssertionError('unrecorded or falsely qualified synthetic scope admitted')
raw=b'[  {"family":"literal-a", "opaque": 7},\n{"family":"literal-b", "opaque": 8} ]'
rows=compose.row_slices(raw);assert rows[0][1]==b'{"family":"literal-a", "opaque": 7}' and rows[1][1]==b'{"family":"literal-b", "opaque": 8}';passed+=1
assert [v for v,raw in rows]==json.loads(b'['+b','.join(raw for v,raw in rows)+b']');passed+=1
print(json.dumps({'pure_controls_passed':passed,'sealed_content_read':False,'native_execution':False,'FINAL_executed':False,'scope_records_are_synthetic_local_values_only':True}))
