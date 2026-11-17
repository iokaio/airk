# SPDX-License-Identifier: Apache-2.0
"""Pure independent source guards. No Torch/model/gradient execution."""
from pathlib import Path
import copy
import importlib.util
import json
import math
import types
HERE=Path(__file__).resolve().parent
p=HERE/'guards.py';g=types.ModuleType('_feature_guard_tests');g.__file__=str(p);exec(compile(p.read_bytes(),str(p),'exec'),g.__dict__)
ids=['independent-train-'+str(i) for i in range(44)]
rows=[]
for i,identifier in enumerate(ids):
 compiled={'input_digest':'native-'+str(i),'input_ids':[1,2,3],'attention_mask':[1,1,1],'position_ids':[0,1,2],'decision_position':1,'candidate_positions':[70],'option_mask':[True]}
 card={'action':{'kind':'read'},'gap':'unsupported','reason':'additional_evidence_may_help','assessments':[{'role':'unknown','relevance':'uncertain'}]}
 row={'id':identifier,'split':'train','status':'raw_scores_only','compiled':compiled,'roster':{'cards':[card]},'scores':[0.25]}
 example={'id':identifier,'native_row_index':i,'native_input_digest':'native-'+str(i),'positive_option_indices':[0],'negative_option_indices':[],'semantic_labels':None}
 rows.append({'original_native_row_index':i,'native_row':row,'example':example})
g.validate_rows(rows,ids);passed=1
def refuses(change):
 global passed
 value=copy.deepcopy(rows);change(value)
 try:g.validate_rows(value,ids)
 except (ValueError,KeyError,TypeError):passed+=1;return
 raise AssertionError('invalid independent training/resource case admitted')
refuses(lambda r:r[0]['native_row'].update(split='calibration'))
refuses(lambda r:r[0]['native_row'].update(split='final'))
refuses(lambda r:r.reverse())
refuses(lambda r:r.pop())
refuses(lambda r:r[0]['native_row']['compiled'].update(input_digest='foreign'))
refuses(lambda r:r[0]['example'].update(negative_option_indices=[0]))
refuses(lambda r:r[0]['example'].update(semantic_labels={'sufficient':True}))
refuses(lambda r:r[0]['native_row']['compiled'].update(input_ids=[1]*2049))
refuses(lambda r:r[0]['native_row']['compiled'].update(decision_position=3))
refuses(lambda r:r[0]['native_row']['compiled'].update(option_mask=[False]))
refuses(lambda r:r[0]['native_row']['roster']['cards'][0].update(action={'kind':'sufficient'}))
refuses(lambda r:r[0]['native_row']['roster']['cards'][0].update(assessments=[{'role':'test','relevance':'useful'}]))
refuses(lambda r:r[0]['example'].update(positive_option_indices=[0,0]))
refuses(lambda r:r[0]['native_row'].update(scores=[float('nan')]))
g.admit_gpu(36,48,32,4);passed+=1
for args in [(35,48,32,4),(36,32,32,4),(0,48,32,4)]:
 try:g.admit_gpu(*args)
 except ValueError:passed+=1
 else:raise AssertionError('GPU resource refusal failed')
g.isolated_output(HERE/'unused-synthetic-output',HERE,[HERE/'frozen-synthetic-input']);passed+=1
for frozen in [HERE,HERE/'unused-synthetic-output',HERE/'unused-synthetic-output'/'nested']:
 try:g.isolated_output(HERE/'unused-synthetic-output',HERE,[frozen])
 except ValueError:passed+=1
 else:raise AssertionError('frozen overlap admitted')
assert 'torch' not in __import__('sys').modules
print(json.dumps({'pure_controls_passed':passed,'torch_imported':False,'neural_execution':False,'gradients':False}))
