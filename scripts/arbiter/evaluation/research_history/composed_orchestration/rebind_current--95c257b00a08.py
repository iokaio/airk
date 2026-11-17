# SPDX-License-Identifier: Apache-2.0
"""Minimal current-artifact metadata rebind; never launches native/model/Cargo or regenerates data."""
import argparse,copy,hashlib,json,os
from pathlib import Path
BASE=Path(__file__).resolve().parent

def need(ok,message):
 if not ok:raise ValueError(message)
def measured(path):
 path=Path(path).absolute();need(path.is_file() and not path.is_symlink(),'Regular retained input required')
 with path.open('rb') as f:sha=hashlib.file_digest(f,'sha256').hexdigest()
 return {'path':str(path),'sha256':sha}
def read(value):
 need(measured(value['path'])==value,'Frozen input changed: '+value['path']);return json.loads(Path(value['path']).read_bytes())
def retain(path,value):
 with path.open('x',encoding='utf-8') as f:json.dump(value,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def main():
 a=argparse.ArgumentParser(description=__doc__)
 for key in ['images','images-sha256','build-receipt','build-receipt-sha256']:a.add_argument('--'+key,required=True)
 a.add_argument('--stage',action='store_true');args=a.parse_args()
 recipe=json.loads((BASE/'recipe.json').read_bytes());inventory={'path':str(Path(args.images).absolute()),'sha256':args.images_sha256};actual_build={'path':str(Path(args.build_receipt).absolute()),'sha256':args.build_receipt_sha256}
 if not args.stage:print(json.dumps({'stage_performed':False,'new_group':recipe['new_group'],'native_or_model':False}));return
 packet=read(inventory);build=read(actual_build);start=read(recipe['frozen_current_build_start']);oldbuild=read(recipe['old_build'])
 need(packet['schema']=='airk.preserved-native-test-images/1' and packet['native_group']==recipe['new_group'] and packet['actual_build_receipt']==actual_build,'Actual current inventory/build differs')
 need(build['passed'] is True and build['exit_code']==0 and build['inputs_unchanged'] is True and build['groups']['native']==start['groups']['native'],'Current successful build must bind exact original frozen198f source')
 prior=oldbuild['groups']['native']['files'];current=build['groups']['native']['files'];changed=sorted(k for k in set(prior)|set(current) if prior.get(k)!=current.get(k))
 need(oldbuild['groups']['native']['sha256']==recipe['old_group'] and changed==recipe['expected_changed_paths'],'Actual complete source delta differs from frozen11paths')
 plan=read(recipe['old_resource_plan']);request=read(plan['native_input']);manifest=read(request['source_manifest']);repo=Path(plan['repository_root']);sources=[]
 for row in manifest['source_files']:
  value=measured(row['path']);relative=Path(row['path']).relative_to(repo).as_posix()
  need(current.get(relative)==value['sha256'],'Actual live source differs from current build: '+relative)
  if relative not in changed:need(value==row,'Unrelated original source drift')
  sources.append(value)
 image=next(v for v in packet['images'] if v['binary_id']=='airk-inference');need(measured(image['path'])['sha256']==image['sha256'],'Preserved current inference image differs')
 output=Path(recipe['future_output_parent']);need(not output.exists(),'Fresh rebind output required');output.mkdir()
 newmanifest=copy.deepcopy(manifest);newmanifest.update(native_build_receipt=actual_build,source_commit=packet['source_commit'],source_files=sources)
 manifestpath=output/'current-source-manifest.private.json';retain(manifestpath,newmanifest)
 newrequest=copy.deepcopy(request);newrequest['source_manifest']=measured(manifestpath)
 requestpath=output/'resource-run-input.private.json';retain(requestpath,newrequest)
 bridge={'schema':'airk.private.reviewed-native-source-bridge/1','status':'Prepared exact source identity bridge; Root final review required, no execution approval','old_native_group':recipe['old_group'],'new_native_group':recipe['new_group'],'old_build_receipt':recipe['old_build'],'new_build_receipt':actual_build,'changes':{k:{'before_sha256':prior.get(k),'after_sha256':current.get(k),'reviewed_scope':recipe['source_scopes'][k]} for k in changed}}
 bridgepath=output/'exact-current-source-bridge.json';retain(bridgepath,bridge)
 seals=plan['input_seals']
 for name in prior:seals.pop(str(repo/name),None)
 seals.pop(plan['native_input']['path'],None)
 for name,sha in current.items():seals[str(repo/name)]=sha
 def seal(value):seals[value['path']]=value['sha256']
 for value in [inventory,actual_build,recipe['old_build'],measured(requestpath),measured(manifestpath),measured(bridgepath),measured(BASE/'recipe.json'),measured(__file__)]:seal(value)
 seal({'path':image['path'],'sha256':image['sha256']})
 unchanged=[];removed=[]
 for row in plan['unchanged_scoring_source_pins']:
  relative=Path(row['path']).relative_to(repo).as_posix()
  if prior.get(relative)==current.get(relative):unchanged.append(row)
  else:removed.append(row)
 plan.update(native_input=measured(requestpath),input_seals=dict(sorted(seals.items())),old_native_build=recipe['old_build'],required_observer_source_changes={name:current[name] for name in changed},unchanged_scoring_source_pins=unchanged,status='Current actual198f resource-only rebind; Root sole launcher after full review')
 planpath=output/'resource20-plan-r2.json';retain(planpath,plan)
 oldcommand=read({'path':str(BASE.parent/'compact-resource20-preparation-r1/command-freeze-r1.json'),'sha256':recipe['old_command_freeze_sha256']})['command'];command=copy.deepcopy(oldcommand)
 replacements={'--plan':str(planpath),'--plan-sha256':measured(planpath)['sha256'],'--image-packet':inventory['path'],'--image-packet-sha256':inventory['sha256'],'--source-bridge':str(bridgepath),'--source-bridge-sha256':measured(bridgepath)['sha256'],'--output':recipe['future_native_output']}
 for flag,value in replacements.items():command[command.index(flag)+1]=value
 report={'schema':'airk.private.compact-resource20-current-rebind/2','status':'FROZEN FOR ROOT REVIEW; NATIVE NEVER LAUNCHED BY REBINDER','plan':measured(planpath),'command':command,'native_input':measured(requestpath),'source_manifest':measured(manifestpath),'source_bridge':measured(bridgepath),'image_inventory':inventory,'actual_build':actual_build,'native_image':image,'native_group':recipe['new_group'],'changed_paths':changed,'source_rows_removed_from_unchanged_claim':removed,'original_limits_unchanged':True,'data_Profile_compiler_graph_expected_input_roster_environment_capacity_watch_owner_unchanged':True,'native_or_model_execution':False}
 retain(output/'command-freeze-r2.json',report);print(json.dumps(report))
if __name__=='__main__':main()
