# SPDX-License-Identifier: Apache-2.0
# PRIVATE: metadata-only r5 freeze; no product execution, no image/model hash.
import copy, hashlib, importlib.util, json, os, shutil, time
from pathlib import Path
S=Path(r'C:\Users\tdotn\AppData\Local\airk-final-exec-20261116T052942Z\fresh-ann-query-r1')
R=Path(r'D:\code\Github\airk')
E=R/'artifacts/v190/final-plan/exec-20261116T052942Z'
P=R/'artifacts/advanced-split/models/actual-composed-service-r2/final-plan-current-policy-preparation-20261116T085632Z/current-orchestration/b2-native-preservation-r1'
spec=importlib.util.spec_from_file_location('r5_metadata_producer',S/'producer.py');p=importlib.util.module_from_spec(spec);spec.loader.exec_module(p)
def need(v,m):
 if not v: raise ValueError(m)
def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def smallpin(path,expected=None):
 path=Path(path);need(path.stat().st_size<2_000_000,'metadata-only size bound')
 pin=p.pin(path)
 if expected:need(pin['sha256']==expected,'authoritative metadata SHA mismatch '+str(path))
 return pin
start=time.monotonic();actor=p.identity()
prior=S/'inputs/r4-prepared.json';need(sha(prior)=='2fa67a709b604ae06b29db8846f91a35a00703a9401288e7cd45fdead87d3663','original prior plan mismatch')
plan=json.loads(prior.read_bytes());old=copy.deepcopy(plan)
packet_path=P/'native-test-images-b2-r1.json'
packetpin=smallpin(packet_path,'3de867c6fda0973f7327c8dfb053c99beb0175e14dc5833ee533c12b5c3e405d')
packet=json.loads(packet_path.read_bytes())
build_path=E/'current-b2-test-image-cargo-json-r1.json'
buildpin=smallpin(build_path,'4115a0590100520891eb46f0e79f1ae5a9d9de917bf41905fe4e720640951b43')
build=json.loads(build_path.read_bytes())
bridge_path=E/'reviewed-native-source-bridge-b2-r1.json'
bridgepin=smallpin(bridge_path,'858cce13e434e8e771d8b1932235cabe0dc92b066994ce16856217afa8c33ad5')
bridge=json.loads(bridge_path.read_bytes())
need(build['passed'] and build['exit_code']==0 and build['inputs_unchanged'] and build['command_inputs_unchanged'],'actual build not unchanged/pass')
ng='18ee803e50e57e63af1e69057b6dcafb1829c69e5cf2d250f89256157e558bbe'
need(packet['native_group']==build['groups']['native']['sha256']==bridge['new_native_group']==ng,'B2 native identity mismatch')
need(packet['source_commit']==bridge['commit']=='dda749cbdf77b3e2106003ff10a683ec3aec3202','B2 source commit mismatch')
need(packet['actual_build_receipt']['sha256']==buildpin['sha256'] and p.unaliased(packet['actual_build_receipt']['path'])==p.unaliased(build_path),'actual build path mismatch')
image=next(x for x in packet['images'] if x['binary_id']=='airk-kernel')
need(image['sha256']=='d5d8cbea443b13c5aa4f5d1dfa2f17c1fa13202a34939fdbc71533d1efe4a14c' and image['bytes']==231956480,'actual B2 Kernel row mismatch')
need(image['preserved_file_owner_sid']==image['original_file_owner_sid']==p.SID,'image human owner receipt mismatch')
need(p.unaliased(image['path'])==p.unaliased(P/'images/airk_kernel-3ae7c75effb71ddd.exe') and Path(image['path']).stat().st_size==image['bytes'],'actual image geometry/size mismatch')
source_rows={}
for rel in ['crates/airk-kernel/src/inference_owner/evaluation_tests/ann_query.rs','crates/airk-kernel/src/inference_owner/evaluation_tests/CONTRACT.md','crates/airk-kernel/src/inference_owner/evaluation_tests.rs']:
 pin=smallpin(R/rel);need(pin['sha256']==build['groups']['native']['files'][rel]==bridge['changes'][rel]['after_sha256'],'owning ANN source not in reviewed B2 build')
 source_rows[rel]=pin
reused=[];refreshed=[]
for key,value in plan['pins'].items():
 if key in ['diagnostic_receipt','diagnostic_image']:continue
 path=Path(value['path'])
 if key not in ['producer','execution_runner']:need(path.stat().st_size==value['bytes'],'unchanged pin size differs '+key)
 if key in ['producer','execution_runner']:
  plan['pins'][key]=smallpin(path);refreshed.append(key)
 elif value['bytes']<2_000_000:
  p.check_pin(value);reused.append(key)
 else:reused.append(key)
plan['name']='r5';plan['actor']=actor
plan['pins']['diagnostic_receipt']=packetpin
plan['pins']['diagnostic_image']={'path':p.native_path(p.unaliased(image['path'])),'bytes':image['bytes'],'sha256':image['sha256']}
plan['pins']['diagnostic_actual_build_receipt']=buildpin
plan['pins']['diagnostic_reviewed_source_bridge']=bridgepin
plan['pins']['path_class_review']=smallpin(E/'fresh-ann-query-r1/full-path-flow-r4/review-receipt.json','b04e642416b796c2445286863b13d4a4db5f9cdabf2f28ae4d112be35cadbe35')
for rel,value in source_rows.items():plan['pins']['diagnostic_source/'+rel]=value
plan['source_groups_new_diagnostic']={k:v['sha256'] for k,v in build['groups'].items()}
plan['metadata_freeze']={'method':'Reuse authentic unchanged r4 pins; validate all small input bytes and sizes, refresh reviewed producer/runner; B2 image digest comes from Root preserved-image packet, not recomputed in this metadata-only phase. Actual runner independently rehashes EVERY pin within the original phase cutoff before launch and again after.','prior_plan':smallpin(prior),'b2_packet':packetpin,'b2_source_commit':packet['source_commit'],'b2_actual_build_receipt':buildpin,'b2_reviewed_source_bridge':bridgepin,'reused_pin_keys':reused,'refreshed_pin_keys':refreshed,'runtime_or_nn_execution':False,'large_image_or_model_hashes':0}
for key in ['source','source_identity','candidate_consumed_c7','candidate_build','profile','native_read_budget','fixture','model_directory','tasks','mode_slots','detail','execution_policy']:
 need(plan[key]==old[key],'unchanged cohort/runtime policy mutated '+key)
need(plan['tasks']==9 and plan['mode_slots']==27,'full original denominator differs')
p.static_inputs()
quality=p.load_script(p.python_source_workspace(plan['source'])/'scripts/check-retrieval-quality.py','r5_metadata_quality')
need(quality.native_read_budget()==plan['native_read_budget'],'Rust emitted deadline changed')
p.preflight_paths(plan,quality)
output=S/'inputs/r5-prepared.json';p.write_new(output,plan)
command=['python','-B','-S',str(S/'execution-owner-r1/runner.py'),'transport','--plan',str(output),'--plan-sha256',sha(output)]
receipt={'schema':'airk.private.fresh-ann-metadata-freeze/1','actor':actor,'plan':smallpin(output),'command':command,'actual_elapsed_seconds':time.monotonic()-start,'complete':True,'source_identity_reused':plan['source_identity'],'candidate_consumed_c7_reused':plan['candidate_consumed_c7'],'diagnostic_image':plan['pins']['diagnostic_image'],'native_source_group':ng,'actual_build_receipt':buildpin,'reviewed_source_bridge':bridgepin,'checked_source_rows':source_rows,'full_path_preflight':True,'fixture_constructor_calls':0,'runtime_or_nn_execution':False,'large_image_or_model_hashes':0,'scope':'Preparation only; actual full input hashing and source equality remain enforced by unchanged reviewed runner within original phase cutoff.'}
receipt_path=S/'inputs/r5-metadata-freeze.json';p.write_new(receipt_path,receipt)
mirror=E/'fresh-ann-query-r1'
for path in [output,receipt_path,Path(__file__)]:
 target=mirror/path.name
 with target.open('xb') as f:f.write(path.read_bytes())
print(json.dumps({'plan':str(output),'sha256':sha(output),'receipt':str(receipt_path),'receipt_sha256':sha(receipt_path),'seconds':receipt['actual_elapsed_seconds'],'command':command,'tasks':9,'mode_slots':27,'runtime_or_nn_execution':False}))