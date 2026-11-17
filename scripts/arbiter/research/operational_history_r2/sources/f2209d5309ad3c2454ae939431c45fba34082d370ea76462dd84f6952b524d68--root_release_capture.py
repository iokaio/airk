# SPDX-License-Identifier: Apache-2.0
import hashlib,json
from pathlib import Path

base=Path(__file__).resolve().parent
proposal=base/'CAL-capture-release.actual.proposed.json'
assert hashlib.sha256(proposal.read_bytes()).hexdigest()=='a0f26c05f2775a52317af3ef3665b3a58ac1e8a31d58557dfc63cfee533d8a58'
value=json.loads(proposal.read_bytes())
assert value['approved'] is None and value['maximum_questions']==100 and value['maximum_probes']==500
assert value['Context_task_forward'] is False
assert value['actual_binding_evidence']['native_source_group']=='3a64a188c7188d44a3da18afd9ca2efa5c3e996f171d28b62bd8a3f89c9f031f'
assert value['frozen_configuration']['revision']==3
for path in value['absent_build_inputs']:
    p=Path(path)
    assert not p.exists() and not p.is_symlink()
value['approved']=True
value['status']='ROOT_APPROVED_EXACT_CAL100_CAPTURE_ONLY'
value['note']='Root reviewed actual native R102 source/image, actual R104 client pair, exact whole CAL source/index/publication and MiniLM pipeline, both selected stages Ready at revision3, component resource receipts and the explicit consumed-source deletion sentinel. Execute only through reviewed owned controller. No release qualification or cold-warm timing pass is granted.'
value['required_unresolved']=[]
value['root_review']={
 'proposal_sha256':'a0f26c05f2775a52317af3ef3665b3a58ac1e8a31d58557dfc63cfee533d8a58',
 'review_type':'Root self-review of integrated execution inputs and independent verification of actual native bindings/receipts',
 'scope':'Exactly the fixed100 CAL questions and at most500 verified counterfactual probes; Focus ON, no Context task forwards, original30s/2048tokens/eightcards and source-authority budgets unchanged. Retain all outcomes and failures. No confirmation data or fitting in this phase.',
 'resource_window':'Exclusive local task/model measurement window; no Cargo, unrelated NN or heavy builds during capture.',
 'readiness':'Actual Ready is a capture prerequisite. Both failed readiness observers remain failed and establish no coldwarm30s claim.',
 'closure':'Owned controller must authenticate exact complete command/image/runtime, enforce its reviewed whole-phase bounds, check absent_build_inputs before/after, reconcile configuration revision3 before/after and retain natural process/session closure. Stop on failure; do not replay unchanged failures.',
 'qualification_granted':False,
}
output=base/'CAL-capture-release.root-approved-r1.json'
assert not output.exists()
output.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'path':str(output),'sha256':hashlib.sha256(output.read_bytes()).hexdigest(),'approved':True}))
