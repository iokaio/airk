# SPDX-License-Identifier: Apache-2.0
"""Pure numeric-owner output guards, independent literal floors. No NN/FINAL."""
from pathlib import Path
import copy,json,types
HERE=Path(__file__).resolve().parent;p=HERE/'run_numeric.py';m=types.ModuleType('_pure_numeric');m.__file__=str(p);exec(compile(p.read_bytes(),str(p),'exec'),m.__dict__)
packet={'expected_families':{'literal-u8':'literal-u8-model','literal-s8':'literal-s8-model'}}
case={'max_component_error':0.000001,'candidate_vs_fp32_cosine':0.999,'candidate_vs_fp32_l2':0.02}
value={'complete':True,'evaluation':True,'families':[{'family':f,'model_json_sha256':model,'candidate_fp32_floor_passed':True,'mixed_length_execution':{'workers':1},'cases':[dict(case) for _ in range(12)]} for f,model in packet['expected_families'].items()]}
m.validate_capture(value,packet);passed=1
changes=[lambda v:v.update(complete=False),lambda v:v['families'].pop(),lambda v:v['families'][0].update(model_json_sha256='foreign'),lambda v:v['families'][0].update(candidate_fp32_floor_passed=False),lambda v:v['families'][0]['cases'].pop(),lambda v:v['families'][0]['mixed_length_execution'].update(workers=2),lambda v:v['families'][0]['cases'][0].update(max_component_error=0.00002),lambda v:v['families'][0]['cases'][0].update(candidate_vs_fp32_cosine=0.994),lambda v:v['families'][0]['cases'][0].update(candidate_vs_fp32_l2=0.11),lambda v:v['families'][0]['cases'][0].update(max_component_error=float('nan')),lambda v:v['families'][0]['cases'][0].update(max_component_error=-1)]
for change in changes:
 v=copy.deepcopy(value);change(v)
 try:m.validate_capture(v,packet)
 except (ValueError,KeyError,TypeError):passed+=1
 else:raise AssertionError('independent invalid numeric output admitted')
print(json.dumps({'pure_controls_passed':passed,'native_execution':False,'FINAL':False}))
