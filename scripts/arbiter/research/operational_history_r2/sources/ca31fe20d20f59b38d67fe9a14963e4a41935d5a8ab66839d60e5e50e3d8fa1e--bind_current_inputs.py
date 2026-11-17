# SPDX-License-Identifier: Apache-2.0
"""Metadata only. Bind actual Cargo artifacts and original Rust Profile masters."""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
MASTERS = (
 ('focus', 'artifacts/advanced-split/focus-native-qualification-preparation-r1/selected-focus-profile-r3/selected-profiles.private.json', '2bb60366ca201e24dcb4674e18664dd57dba96c8ec3518aaf28a61b90849157f'),
 ('compact', 'artifacts/advanced-split/models/qwen3-challengers-r1/context-native-owned-r1/selected-context-17-profile-r1/selected-profiles.private.json', '0ca31515b2a49ac7a8dbcf137ccb545286f7381461f959b79684aa7f04acb896'),
 ('large', 'artifacts/advanced-split/models/qwen3-challengers-r1/context-native-owned-r2/selected-context-4-profile-r2/selected-profiles.private.json', 'd317ee35f476b75c6f51ce5c4d884ab46bcd52ac1b73b5e55b09e23beaeb8974'))

def digest(path):
    value = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024*1024), b''):
            value.update(block)
    return value.hexdigest()

def reference(path):
    path = Path(path).resolve()
    return {'path':str(path),'sha256':digest(path),'bytes':path.stat().st_size}

def main():
    receipt = ROOT/'artifacts/advanced-split/receipts/stage-integration-r97/airk-service-image.json'
    log = receipt.with_suffix('.log')
    command = json.loads(receipt.read_text(encoding='utf-8'))
    if command['exit_code'] != 0 or command['inputs_unchanged'] is not True or command['command_inputs_unchanged'] is not True:
        raise ValueError('actual R97 service build receipt failed/drifted')
    events=[]
    for line in log.read_text(encoding='utf-8').splitlines():
        if line.startswith('{'):
            events.append(json.loads(line))
    if not any(e.get('reason')=='build-finished' and e.get('success') is True for e in events):
        raise ValueError('actual Cargo success event absent')
    images=[e for e in events if e.get('reason')=='compiler-artifact' and e.get('target',{}).get('name')=='airk_service' and e.get('profile',{}).get('test') is True and e.get('executable')]
    if len(images)!=1:
        raise ValueError('exact one actual service test artifact required')
    image=images[0]
    source='crates/airk-service/src/service_runtime/evaluation_fixture_tests.rs'
    if digest(ROOT/source)!=command['groups']['native']['files'][source]:
        raise ValueError('current fixture source differs from R97 compile')
    current=(ROOT/source).read_text(encoding='utf-8')
    raw={}
    for stage,path,pin in MASTERS:
        path=ROOT/path
        if digest(path)!=pin:raise ValueError('actual native master changed: '+stage)
        master=json.loads(path.read_text(encoding='utf-8'))
        if master['schema']!='airk.private.selected-native-profile-master/1' or len(master['profiles'])!=1:
            raise ValueError('actual native single-profile master required')
        row=master['profiles'][0]
        if digest(row['profile'])!=row['profile_sha256']:raise ValueError('actual native Profile changed')
        profile=json.loads(Path(row['profile']).read_text(encoding='utf-8'))
        if profile['manifest']!=row['manifest'] or profile['model']!=row['model'] or row['numeric_raw_only'] is not True or row['qualification'] is not None:
            raise ValueError('raw native metadata identity differs')
        consumer=json.loads(Path(row['consumer_input']).read_text(encoding='utf-8'))
        if consumer['profile']!=row['profile'] or consumer['files']!=row['files']:
            raise ValueError('native consumer overlay changed')
        raw[stage]={'master':reference(path),'profile':reference(row['profile']),'consumer_input':reference(row['consumer_input']),
            'manifest':row['manifest'],'model':row['model'],'files':row['files'],'fixtures':consumer['fixtures'],
            'cpu_fit':profile['cpu_fit'],'memory_reservation_bytes':profile['memory_reservation_bytes'],
            'numeric_raw_only':True,'qualification':None}
    anchor=ROOT/'artifacts/advanced-split/models/actual-composed-service-r1'
    result={'schema':'airk.private.composed-current-bindings/1','state':'metadata_bound_not_executed',
      'service_r97':{'image':reference(image['executable']),'cargo_artifact':image,'build_receipt':reference(receipt),'cargo_log':reference(log),
       'native_source_group':command['groups']['native']['sha256'],'fixture_source':reference(ROOT/source),
       'vector_selector_present':'fn emit_private_selected_vector_configuration()' in current,
       'synthetic_selector_present':'fn emit_private_selected_synthetic_fixture()' in current},
      'current_raw_profiles':raw,'future_fitted_profiles':{'compact':None,'large':None},
      'matched_runtime_pair':None,'selected_minilm':None,'final_three_staged_inputs':None,
      'future_paths':{'private_anchor':str(anchor),'included':str(anchor/'included'),'excluded':str(anchor/'excluded'),
       'vector_drafts':str(anchor/'vector-drafts'),'staged_raw':str(anchor/'staged-raw'),
       'staged_final':str(anchor/'staged-final'),'work_raw':str(anchor/'work-raw'),'work_final':str(anchor/'work-final'),
       'index_included':str(anchor/'index-included'),'index_excluded':str(anchor/'index-excluded'),
       'logs':str(anchor/'logs'),'control_prefix_included':'composed-included-r1','control_prefix_excluded':'composed-excluded-r1'},
      'synthetic_requires_new_image':True,
      'scope':'Metadata only. Raw profiles do not qualify final fitted RAM/load/latency; fitted identities remain unfilled. No model assets copied/rehashed, no native process/session/NN executed.'}
    output=HERE/'current-bindings.actual.json'
    with output.open('x',encoding='utf-8') as stream:stream.write(json.dumps(result,indent=2)+'\n')
    print(json.dumps({'bindings':str(output),'sha256':digest(output),'r97_image_sha256':result['service_r97']['image']['sha256'],
                      'r97_synthetic_present':result['service_r97']['synthetic_selector_present']}))

if __name__=='__main__':main()
