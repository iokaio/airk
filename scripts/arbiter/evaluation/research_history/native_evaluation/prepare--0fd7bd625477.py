# SPDX-License-Identifier: Apache-2.0
"""Prepare path/hash-only Rust fitter configuration after all four natural RAW passes.

No scores, thresholds, native payloads, model execution or activation are produced.
"""
import hashlib,json
from pathlib import Path
root=Path(__file__).resolve().parents[4];out=Path(__file__).resolve().parent
def sha(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
seal_path=root/'artifacts/advanced-split/models/context-action-raw-profile-seal-r23/asset-seal.json'
assert sha(seal_path)=='4a922b12e2793c041f31b9381a2403ed420263f09f891880e7faba050f71db05'
seal=json.loads(seal_path.read_text(encoding='utf-8'))
descriptor=root/'artifacts/advanced-split/models/native-weight8-mixed-cpu8-cpu24-profiles-r1/inputs.json'
assert sha(descriptor)=='ae0f2bd9f19e374190cf0afdb2b86100f7d53933d0654801c1f812be5e616474'
entries={entry['profile']:entry for entry in json.loads(descriptor.read_text(encoding='utf-8'))}
names={'arbiter-small-intern-08b':'context-action-smallintern-raw-r24','arbiter-small-strands-2b':'context-action-strands-raw-r24','arbiter-large-intern-4b':'context-action-intern4-raw-r24','arbiter-large-kev-4b':'context-action-kev4-raw-r24'}
preflight=root/'artifacts/advanced-split/models/context-action-native-preflight-empty-r24/reference-preflight.private.json'
assert sha(preflight)=='bef6e957646b68d38ddbe3b3dccb7fc3fe4722479d330e87c89e11a866da60ff'
closure=seal['sha256_pins'].copy();models=[]
for model in seal['models']:
    directory=root/'artifacts/advanced-split/models'/names[model['model']]
    raw=directory/'raw-scores.private.json';execution=directory/'execution.json'
    receipt=json.loads(execution.read_text(encoding='utf-8'));owned=receipt['owned']
    assert owned['exit_code']==0 and owned['owned_descendants_settled'] and owned['owned_job_final_active_processes']==0 and not owned['owned_job_forced_termination']
    assert not receipt['post_input_drift']
    value=json.loads(raw.read_text(encoding='utf-8'))
    assert value['complete'] and value['model']==model['model'] and len(value['rows'])==126
    assert not value['task_qualified'] and not value['release_qualified']
    profile=Path(model['profile']);assert sha(profile)==model['profile_source_sha256']
    models.append({'profile':str(profile),'profile_sha256':model['profile_source_sha256'],'raw':str(raw),'raw_sha256':sha(raw),'base_assets':entries[str(profile)]['assets']})
    closure[str(raw)]=sha(raw);closure[str(execution)]=sha(execution)
facts=root/'artifacts/advanced-split/models/context-action-publisher-facts-r1/publisher-facts.private.json'
capture=root/'artifacts/advanced-split/models/context-action-native-capture-r23/native-capture.private.json'
for path,expected in [(facts,'115f9baacef8d9cac1c9602447a854a11787ed759e2f61897eb01009ae6d4254'),(capture,'725f3ec7950c7371cc3663d0d3c1cb3475d4674f1104d994002274b7bf3e1105'),(preflight,'bef6e957646b68d38ddbe3b3dccb7fc3fe4722479d330e87c89e11a866da60ff')]:
    assert sha(path)==expected;closure[str(path)]=expected
config={'preflight':str(preflight),'preflight_sha256':closure[str(preflight)],'models':models}
with (out/'fit-input.json').open('x',encoding='utf-8') as stream:stream.write(json.dumps(config,indent=2)+'\n')
closure[str(out/'fit-input.json')]=sha(out/'fit-input.json')
with (out/'input-closure.json').open('x',encoding='utf-8') as stream:stream.write(json.dumps({'schema':'airk.private.context-action-fit-input-closure/1','sha256_pins':closure,'all_model_asset_bytes_must_be_rechecked_before_and_after_fit':True,'fit_or_nn_executed':False,'activation':False},indent=2)+'\n')
print(json.dumps({'fit_input':str(out/'fit-input.json'),'sha256':sha(out/'fit-input.json'),'closure':str(out/'input-closure.json'),'closure_sha256':sha(out/'input-closure.json'),'models':len(models),'fit_executed':False}))
