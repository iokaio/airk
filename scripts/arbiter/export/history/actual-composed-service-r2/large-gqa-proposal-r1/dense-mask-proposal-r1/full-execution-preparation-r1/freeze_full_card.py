# SPDX-License-Identifier: Apache-2.0
"""Freeze actual Dense outputs into the maintained one-forward card; never launch."""
from pathlib import Path
import argparse,copy,hashlib,json,math,os
R=Path(r'D:\code\Github\airk');B=R/'artifacts/advanced-split/models/actual-composed-service-r2';JL=B/'large-gqa-proposal-r1';JD=JL/'dense-mask-proposal-r1';BASE=Path(__file__).resolve().parent
SID='S-1-5-21-3678370870-3858642417-826968105-1002'
def sha(p):
    with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def pin(p):p=Path(p);return {'path':str(p),'bytes':p.stat().st_size,'sha256':sha(p)}
def checked(v):p=Path(v['path']);assert p.is_file() and not p.is_symlink() and sha(p)==v['sha256'];return p
def write(p,v):
    with p.open('x',encoding='utf-8',newline='\n') as f:json.dump(v,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def main():
    a=argparse.ArgumentParser(description=__doc__)
    for name in ['images','images-sha256','tiny17','tiny17-sha256','tiny128','tiny128-sha256','tiny-review','tiny-review-sha256','source-bridge','source-bridge-sha256']:a.add_argument('--'+name,required=True)
    x=a.parse_args();packetpin={'path':x.images,'sha256':x.images_sha256};packet=json.loads(checked(packetpin).read_bytes());buildpin=packet['actual_build_receipt'];build=json.loads(checked(buildpin).read_bytes());assert build['passed'] is True and build['inputs_unchanged'] is True and packet['native_group']==build['groups']['native']['sha256']
    image=next(v for v in packet['images'] if v['binary_id']=='airk-inference');assert image['preserved_file_owner_sid']==image['original_file_owner_sid']==SID;checked(image)
    metadata=JD/'dense-native-metadata-r1';nativepath=metadata/'metadata/arbiter-context-qwen3-4b/resource-run-input.private.json';native=json.loads(nativepath.read_bytes());manifest=json.loads(checked(native['source_manifest']).read_bytes());assert 'large_dense_graph' in manifest and manifest['native_build_receipt']['sha256']==buildpin['sha256'];profile=json.loads(checked(native['profile']).read_bytes());assert profile['model']=='arbiter-context-qwen3-4b' and profile['max_tokens']==8192 and profile['memory_reservation_bytes']==40<<30
    blank=metadata/'blank-template-large.json';oldblank=B/'final-plan-current-policy-preparation-20261116T085632Z/nn0-native-preparation-r1/arbiter-context-qwen3-4b/template.json';assert blank.read_bytes()==oldblank.read_bytes()
    original=R/'artifacts/advanced-split/models/weights-v2/software-v2-independent-numeric-r1/runs-native-r1/revision-r2/assembled-large/numeric-fixture.json';assert sha(original)=='c87c4edcdec2ab5e98b41bfc186c3bb7d60791e7b9efd0aea186b731eecc3dd2';original_body=json.loads(original.read_bytes());fixture=copy.deepcopy(original_body);template=json.loads(blank.read_bytes());assert len(fixture['rows'])==len(template['rows'])==3
    for row,expected in zip(fixture['rows'],template['rows']):assert row['name']==expected['name'] and row['input']==expected['input'];row['reference_same_artifact']=None
    probe=copy.deepcopy(fixture)
    for row,old in zip(probe['rows'],original_body['rows']):row['reference_same_artifact']=old['reference_same_artifact']
    assert probe==original_body and all(all(math.isfinite(v) for v in row['reference_original_fp32']) for row in fixture['rows'])
    owner_card=json.loads((BASE/'owner/preparation.json').read_bytes());root=JD/'full-forward-card-r1';assert not root.exists();root.mkdir();fixturepath=root/'numeric-fixture-original-only.private.json';write(fixturepath,fixture)
    request=copy.deepcopy(native);assert request['numeric_fixture'] is None;request['numeric_fixture']=copy.deepcopy(native['profile']);request['numeric_fixture'].update(path=str(fixturepath),sha256=sha(fixturepath));inputpath=root/'resource-run-input-original-only.private.json';write(inputpath,request)
    tiny=[]
    for path,h,t in [(x.tiny17,x.tiny17_sha256,17),(x.tiny128,x.tiny128_sha256,128)]:
        op={'path':path,'sha256':h};owned=json.loads(checked(op).read_bytes());assert owned['complete'] is True and not owned['post_input_drift'] and not owned['original_phase_expired']
        bodypin=owned['pins']['body'];body=json.loads(checked(bodypin).read_bytes());assert body['complete'] is True and body['tokens']==t and len(body['controls'])==3
        gate={'owner':op,'body':bodypin,'profiling':[],'optimized':[]}
        for index in range(3):
            for kind in ['profiling','optimized']:
                p=owned['pins'][f'control-{index}-{kind}'];checked(p);gate[kind].append(copy.deepcopy(p))
        tiny.append(gate)
    tinyreview={'path':x.tiny_review,'sha256':x.tiny_review_sha256};review=json.loads(checked(tinyreview).read_bytes());assert review['schema']=='airk.private.reviewed-gqa-native-dense-tiny-gates/1' and review['complete'] is True and review['native_image_sha256']==image['sha256']
    bridgepin={'path':x.source_bridge,'sha256':x.source_bridge_sha256};bridge=json.loads(checked(bridgepin).read_bytes());assert bridge['schema']=='airk.private.reviewed-native-source-bridge/1' and bridge['old_native_group']==bridge['new_native_group']==packet['native_group'] and bridge['changes']=={}
    old=json.loads((B/'compact-native-diagnostic-owner-r1/full-model-plan-r3.json').read_bytes());plan={k:copy.deepcopy(v) for k,v in old.items() if k!='input_seals'}
    plan.update(schema='airk.private.large-dense-profiled-native-plan/1',diagnostic_measurement_identity='large-fp32-explicit-mask-dense-profiled/1',limits=owner_card['limits'],native_input=pin(inputpath),profile=profile['manifest'],profile_id=profile['profile_id'],cpu_fit=profile['cpu_fit'],native_policy=native['policy'],native_policy_digest=native['policy_digest'],numeric_input_digest=fixture['rows'][2]['input']['input_digest'],runtime_manifest=owner_card['runtime_manifest'],image_packet=packetpin,source_bridge=bridgepin,old_build=buildpin,structural_proof=pin(JD/'dense-mask-graph-r1/model.proof.json'),optimized_tiny_review=tinyreview,tiny_gates=tiny,output_root=str(JD/'dense-native-profiled-execution-r1'))
    assert not Path(plan['output_root']).exists();seals={}
    def seal(v):seals[v['path']]=v['sha256']
    for rel,h in build['groups']['native']['files'].items():assert sha(R/rel)==h;seals[str(R/rel)]=h
    for v in owner_card['source_pins'].values():checked(v);seal(v)
    for v in [pin(BASE/'owner/preparation.json'),plan['native_input'],packetpin,bridgepin,buildpin,plan['structural_proof'],tinyreview,plan['capacity_os_control'],native['profile'],native['source_manifest'],request['numeric_fixture'],pin(original),pin(blank),pin(oldblank),pin(JD/'dense-mask-graph-r1/owned-execution.json')]:checked(v);seal(v)
    for v in manifest['source_files']+list(manifest['large_dense_graph']['candidate'].values())+list(manifest['large_dense_graph']['prior'].values()):checked(v);seal(v)
    for gate in tiny:
        for v in [gate['body'],gate['owner'],*gate['profiling'],*gate['optimized']]:checked(v);seal(v)
    checked(image);seal(image)
    for asset in profile['assets']:
        p=native['files'][asset['name']];assert Path(p).stat().st_size==asset['bytes'];seals[p]=asset['sha256']
    producer=json.loads(checked(fixture['reference_producer']).read_bytes());seal(fixture['reference_producer'])
    for v in producer['reference_executions']+[producer['native_template'],producer['original_source_roster']]:checked(v);seal(v)
    for kind in ['original_fp32','same_artifact']:seal(pin(original.parent.parent/f'large-{kind}/torch-output.json'))
    plan['required_native_sources']={rel:build['groups']['native']['files'][rel] for rel in ['crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/attention_diagnostic.rs','crates/airk-retrieval/src/arbiter/runtime.rs','crates/airk-retrieval/src/arbiter/runtime/qualification/regeneration.rs','crates/airk-retrieval/src/arbiter/runtime/qualification/regeneration/compact_graph.rs','crates/airk-retrieval/src/arbiter/runtime/dense_synthetic_tests.rs']};seals[str(Path(__file__).resolve())]=sha(Path(__file__).resolve());plan['input_seals']=seals
    path=root/'full-model-plan-r1.json';write(path,plan);argv=[owner_card['parent_python']['path'],'-B','-S','-X','pycache_prefix='+str(BASE/'owner/never-created-runtime-cache'),str(BASE/'owner/runner_dense_profile_only.py'),'--plan',str(path),'--plan-sha256',sha(path),'--execute-reviewed'];write(root/'review.json',{'plan':pin(path),'argv':argv,'native_group':packet['native_group'],'actual_build_ancestry':packet['source_commit'],'blank_byte_equal':True,'fixture_only_delta':'3sameartifact refsnull, exactoriginalfullinputs/FP32 arrays/headers/producer unchanged','native_input_only_delta':'numeric_fixture attached with copiedRustCorepinshape','source_seals':len(seals),'native_model_or_runtime_run':False});print(json.dumps({'plan':pin(path),'argv':argv,'native_executed':False}))
if __name__=='__main__':main()
