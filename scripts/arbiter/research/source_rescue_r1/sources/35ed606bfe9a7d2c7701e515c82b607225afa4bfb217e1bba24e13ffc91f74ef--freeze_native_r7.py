# SPDX-License-Identifier: Apache-2.0
import copy,hashlib,importlib.util,json,sys,time
from pathlib import Path
R=Path(r'D:\code\Github\airk');E=R/'artifacts/v190/final-plan/exec-20261116T052942Z';S=Path(r'C:\Users\tdotn\AppData\Local\airk-final-exec-20261116T052942Z\fresh-ann-query-r1');D=S/'execution-owner-r1';sys.path.insert(0,str(D))
spec=importlib.util.spec_from_file_location('r7_metadata_producer',S/'producer.py');p=importlib.util.module_from_spec(spec);spec.loader.exec_module(p)
def need(v,m):
 if not v:raise ValueError(m)
def smallpin(path,expected=None):
 need(Path(path).stat().st_size<2_000_000,'Metadata-only size bound')
 value=p.pin(path)
 if expected:need(value['sha256']==expected,'Authoritative metadata hash differs')
 return value
started=time.monotonic();actor=p.identity()
prior=S/'inputs/r6-prepared.json';priorpin=smallpin(prior,'5a7a35fcf97287b9aa17f2780d077dad2c6683e024e6576682cf7b5c28f609a0');plan=json.loads(prior.read_bytes());old=copy.deepcopy(plan)
packetpath=R/'artifacts/advanced-split/models/actual-composed-service-r2/final-plan-current-policy-preparation-20261116T085632Z/current-orchestration/b3-native-preservation-r1/native-test-images-b3-r1.json'
packetpin=smallpin(packetpath,'003d9cb7c6475ef9f6f40c54ae7c2944e61f32de6bdfd3750548058116dc5c94');packet=json.loads(packetpath.read_bytes())
buildpath=Path(packet['actual_build_receipt']['path']);buildpin=smallpin(buildpath,'566b3ef8c6989f2182b483aa0039af933e88b5a567bc7d9b6e264ae5cc61dca6');build=json.loads(buildpath.read_bytes())
bridgepath=E/'reviewed-native-source-bridge-b3-r1.json';bridgepin=smallpin(bridgepath,'2607dfe56629571380afdbd3f8a7d550ca4517647480f26c5879706a82e30999');bridge=json.loads(bridgepath.read_bytes())
ng='3f65d079ac09f33d53f90e71be70c6fb7cd05b2a2261507b4f70c8dad4348bd4';need(packet['native_group']==build['groups']['native']['sha256']==bridge['new_native_group']==ng,'B3 group mismatch')
need(bridge['old_native_group']=='18ee803e50e57e63af1e69057b6dcafb1829c69e5cf2d250f89256157e558bbe','Original B2 bridge mismatch')
need(build['passed'] and build['exit_code']==0 and build['inputs_unchanged'],'Actual source-bound build not pass/unchanged')
need(packet['actual_build_receipt']['sha256']==buildpin['sha256'],'Preserved image build mismatch')
image=next(x for x in packet['images'] if x['binary_id']=='airk-kernel');need(image['sha256']=='8ce7376a625bb3677d147b2e997ac9d8f408fdc1a0323581eb2ff69d0d481db5' and image['bytes']==232614912,'Actual B3 Kernel row mismatch')
need(image['preserved_file_owner_sid']==image['original_file_owner_sid']==p.SID and Path(image['path']).stat().st_size==image['bytes'],'Actual B3 metadata human/size mismatch')
changedsource={}
need(len(bridge['changes'])==12,'Reviewed source bridge path count mismatch')
for rel,change in bridge['changes'].items():
 if change['after_sha256'] is None:need(not (R/rel).exists(),'Removed reviewed source remains');continue
 value=smallpin(R/rel,change['after_sha256']);need(build['groups']['native']['files'].get(rel)==value['sha256'],'Bridge path absent/changed in actual native build');changedsource[rel]=value
preserved=[];refreshed=[]
for key,value in plan['pins'].items():
 if key.startswith('diagnostic_source/'):
  rel=key[len('diagnostic_source/'):];current=smallpin(R/rel,build['groups']['native']['files'][rel]);plan['pins'][key]=current;refreshed.append(key)
 elif key not in ['diagnostic_receipt','diagnostic_image','diagnostic_actual_build_receipt','diagnostic_reviewed_source_bridge']:
  need(Path(value['path']).stat().st_size==value['bytes'],'Unchanged input size differs: '+key)
  if value['bytes']<2_000_000:p.check_pin(value)
  preserved.append(key)
for key in ['diagnostic_receipt','diagnostic_actual_build_receipt','diagnostic_reviewed_source_bridge']:plan['pins']['prior_b2_'+key]=copy.deepcopy(old['pins'][key])
plan['pins']['diagnostic_receipt']=packetpin;plan['pins']['diagnostic_actual_build_receipt']=buildpin;plan['pins']['diagnostic_reviewed_source_bridge']=bridgepin
plan['pins']['diagnostic_image']={'path':p.native_path(p.unaliased(image['path'])),'bytes':image['bytes'],'sha256':image['sha256']}
for rel,value in changedsource.items():plan['pins']['reviewed_b3_source/'+rel]=value
seed=S/'inputs/r5-seed.json';seedpin=smallpin(seed,'46b6a447c625c26f32a1555f6fcc076443d345ed98ee557994cd764e501e60e1')
plan['name']='r7';plan['actor']=actor;plan['source_groups_new_diagnostic']={k:v['sha256'] for k,v in build['groups'].items()}
plan['metadata_freeze']={'method':'Native-only B3 derivative of authentic reviewed r6; unchanged r5transport/seed and all original runtime/model/source closure identities retained. New diagnostic image SHA from Root preserved-image packet (not rehashed here); all12 reviewed B2-to-B3 source changes and active ANN owners match actual B3 build. Original runner retains full pin/source hashes inside original120s phase before/after launch.','prior_plan':priorpin,'large_image_or_model_hashes':0,'preserved_pin_keys':preserved,'refreshed_active_b3_source_pin_keys':refreshed,'b3_packet':packetpin,'b3_actual_build':buildpin,'b2_to_b3_bridge':bridgepin,'new_transport_index_or_service':False}
plan['native_only_handoff']['r6_original_plan']=priorpin;plan['native_only_handoff']['preserved_r6_capture_shape_refusal']=smallpin(E/'fresh-ann-query-r1/native-pins-r6-failure/failure-review.json','43b3567a0b1f808550709386fd0d9263dce9fb163ff68737f67d1e82d2c77768')
for key in ['source','source_identity','candidate_consumed_c7','candidate_build','profile','native_read_budget','fixture','model_directory','tasks','mode_slots','detail','execution_policy']:
 need(plan[key]==old[key],'Preserved original source/cohort/model/policy differs '+key)
need(plan['tasks']==9 and plan['mode_slots']==27,'Original denominator differs')
rspec=importlib.util.spec_from_file_location('r7_reviewed_native_runner',D/'native-runner-r6.py');runner=importlib.util.module_from_spec(rspec);rspec.loader.exec_module(runner)
env=runner.native_environment_paths(seed,S/'evidence/r7-native-pins.jsonl',p)
runner.phase_output_paths(plan,'native-pins');runner.phase_output_paths(plan,'native-compare')
need(p.sha(D/'runner.py')==old['pins']['execution_runner']['sha256'] and p.sha(D/'native-runner-r6.py')==old['pins']['native_execution_runner_r6']['sha256'],'Preserved original/additive runner changed')
output=S/'inputs/r7-prepared.json';p.write_new(output,plan)
cmd=['python','-B','-S',str(D/'native-runner-r6.py'),'native-pins','--plan',str(output),'--plan-sha256',p.sha(output),'--input',str(seed),'--input-sha256',seedpin['sha256']]
receipt={'schema':'airk.private.fresh-ann-b3-native-plan/1','actor':actor,'plan':smallpin(output),'command':cmd,'seed':seedpin,'packet':packetpin,'actual_build':buildpin,'source_bridge':bridgepin,'all12_reviewed_b3_source_pins':changedsource,'diagnostic_image':plan['pins']['diagnostic_image'],'actual_native_env_dry':env,'original_9_queries_27_transport_slots_retained':True,'actual_transport_repeated':False,'large_image_or_model_hashes':0,'elapsed_seconds':time.monotonic()-started,'runtime_native_or_nn_executed':False,'review_scope':'B3 image and source identity update only. Original r5/r6 plans/failures/raw27capture/seed/Stop/process proofs unchanged. Native pin helper must genuinely admit original lease and emit full native head/model; comparison consumes literal Rust emission only.'}
receiptpath=S/'inputs/r7-native-plan-freeze.json';p.write_new(receiptpath,receipt)
mirror=E/'fresh-ann-query-r1/native-plan-r7';mirror.mkdir(exist_ok=False)
for path in [output,receiptpath,Path(__file__)]:
 with (mirror/path.name).open('xb') as h:h.write(path.read_bytes())
print(json.dumps({'plan':smallpin(output),'receipt':smallpin(receiptpath),'elapsed_seconds':receipt['elapsed_seconds'],'command':cmd,'actual_execution':False}))