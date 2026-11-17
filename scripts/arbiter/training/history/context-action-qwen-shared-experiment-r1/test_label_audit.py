# SPDX-License-Identifier: Apache-2.0
"""Independent byte/provenance/cold-scope controls; no model, CAL or FINAL content."""
from pathlib import Path
import copy,json,types
from unittest.mock import patch
HERE=Path(__file__).resolve().parent
p=HERE/'prepare_train_label_queue.py';audit=types.ModuleType('_pure_selected_label_audit');audit.__file__=str(p);exec(compile(p.read_bytes(),str(p),'exec'),audit.__dict__)
d=json.loads((HERE/'label-audit-inputs.json').read_bytes());reader=audit.captured_module(d['inputs']['cold_reader'],'_pure_selected_cold_reader');passed=0
source=json.dumps({'tasks':[{'id':'literal-train','split':'train','dataset':'QASPER','text':'openTRAIN'},{'id':'held','split':'test','dataset':'QASPER','text':'CLOSED_NONTRAIN_SENTINEL'},{'id':'cal','split':'calibration','dataset':'QASPER','text':'CLOSED_NONTRAIN_SENTINEL'}]}).encode();original=json.loads
def coldguard(raw,*args,**kwargs):
 if 'CLOSED_NONTRAIN_SENTINEL' in (raw.decode() if isinstance(raw,bytes) else raw):raise AssertionError('nonTRAIN field decoded')
 return original(raw,*args,**kwargs)
with patch.object(reader.json,'loads',coldguard):rows=audit.cold_rows(reader,source,'tasks')
assert [r['id'] for r in rows]==['literal-train'];passed+=1
for intervals,wanted in [([(0,192)],False),([(0,192),(193,300)],False),([(0,192),(192,300)],True),([(0,300)],True)]:assert audit.covered(intervals,0,300)==wanted;passed+=1
assert audit.covered([(0,0)],0,0) is False;passed+=1
alt={'unanswerable':False,'alignment':'all_original_evidence_maps_to_allowed_text','evidence':[{'allowed_locations':['a']},{'allowed_locations':['b']}]};spans=[{'document':'a','start':0,'end':3},{'document':'b','start':3,'end':6},{'document':'distractor','start':6,'end':9}]
full=audit.support_certificate(alt,[(0,3),(3,6)],spans,0);assert full['reference_complete'] and full['sufficient_reference_projection'] and full['full_card_semantic_gold'] is False;passed+=1
r=audit.support_certificate(alt,[(0,3),(3,6),(6,9)],spans,0);assert r['reference_complete'] and not r['sufficient_reference_projection'];passed+=1
r=audit.support_certificate(alt,[(0,3)],spans,0);assert not r['reference_complete'];passed+=1
r=audit.support_certificate(alt,[(0,6)],spans,0);assert r['reference_complete'] and not r['sufficient_reference_projection'];passed+=1
r=audit.support_certificate(alt,[(0,3),(3,6)],spans,1);assert not r['sufficient_reference_projection'];passed+=1
for key,val in [('unanswerable',True),('alignment','partial')]:a=copy.deepcopy(alt);a[key]=val;assert audit.support_certificate(a,[(0,3),(3,6)],spans,0)['reference_complete'] is None;passed+=1
assert audit.mapped('café','α café z'.encode())==(3,8);passed+=1
assert audit.mapped('same',b'same same') is None;passed+=1
assert audit.mapped('missing',b'other') is None;passed+=1
print(json.dumps({'pure_controls_passed':passed,'NN':False,'CAL_or_FINAL_contents_read':False,'human_labels_created':0}))
