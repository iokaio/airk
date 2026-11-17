# SPDX-License-Identifier: Apache-2.0
"""Pure human-provenance/scope guard controls. Synthetic local values, not human labels."""
from pathlib import Path
import copy,json,types
HERE=Path(__file__).resolve().parent;p=HERE/'validate_human_responses.py';v=types.ModuleType('_pure_human_guard');v.__file__=str(p);exec(compile(p.read_bytes(),str(p),'exec'),v.__dict__)
case={'id':'synthetic','native_input_digest':'literal-view','model_visible_current192_units':[{'alias':'e1'}],
 'whole_cards':[{'symbol':'A','native_card':{'id':'read-card','action':{'kind':'read'}}},{'symbol':'B','native_card':{'id':'suff-card','action':{'kind':'sufficient'}}}],
 'original_human_reference_for_reviewer_only':[{'unanswerable':False}]}
response={'id':'synthetic','native_input_digest':'literal-view','recorded_at_UTC':'SYNTHETIC-NOT-ACTUAL-ANNOTATION','checked_entire_model_visible_context':True,
 'whole_cards':[{'symbol':'A','native_card_id':'read-card','judgment':'rejected','checked_entire_card_statement':True,'rationale':'SYNTHETIC'},{'symbol':'B','native_card_id':'suff-card','judgment':'acceptable','checked_entire_card_statement':True,'rationale':'SYNTHETIC'}],
 'no_valid_card':False,'shown_context_sufficient':True,'expected_supported_answer':'literal','citation_aliases':['e1'],'finalized_by_human':True}
assert v.assess_case(case,response)['semantic_gold_eligible'];passed=1
for change in [lambda r:r.update(native_input_digest='foreign'),lambda r:r['whole_cards'].pop(),lambda r:r.update(checked_entire_model_visible_context=False),lambda r:r['whole_cards'][1].update(checked_entire_card_statement=False),lambda r:r.update(no_valid_card=True),lambda r:r.update(citation_aliases=['unseen']),lambda r:r.update(expected_supported_answer=None)]:
 r=copy.deepcopy(response);change(r)
 try:v.assess_case(case,r)
 except (ValueError,KeyError,TypeError):passed+=1
 else:raise AssertionError('invalid source/wholecard/support claim admitted')
r=copy.deepcopy(response);r['whole_cards'][0]['judgment']='unjudged';assert not v.assess_case(case,r)['semantic_gold_eligible'];passed+=1
r=copy.deepcopy(response);r['finalized_by_human']=None;assert not v.assess_case(case,r)['semantic_gold_eligible'];passed+=1
c=copy.deepcopy(case);c['original_human_reference_for_reviewer_only']=[{'unanswerable':True}];assert not v.assess_case(c,response)['semantic_gold_eligible'];passed+=1
q={'episodes':[case]};packet={'schema':'airk.private-human-whole-card-responses/1','queue_sha256':'literal-queue','scope':'TRAIN_RUBRIC_PILOT_ONLY_NOT_CAL_OR_HELDOUT','reviewer':{'id':None,'human':None,'provenance':None,'blind_to_selected_Context_model_outputs':None},'cases':[response]}
try:v.validate(q,packet,'literal-queue')
except ValueError:passed+=1
else:raise AssertionError('blank reviewer became human approval')
print(json.dumps({'pure_controls_passed':passed,'human_labels_created':0,'real_human_approval_claimed':False,'NN':False}))
