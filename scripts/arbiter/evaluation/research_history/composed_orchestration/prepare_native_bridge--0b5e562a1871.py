# SPDX-License-Identifier: Apache-2.0
"""Prepare native same-weight metadata bridge. Default describes only; no Core DTOs."""
import argparse,copy,hashlib,importlib.util,json,os,subprocess,sys,time
from pathlib import Path
from types import SimpleNamespace
HERE=Path(__file__).resolve().parent
REPO=Path('D:/code/Github/airk')
PRIOR=HERE.parent
R3=REPO/'artifacts/advanced-split/models/actual-composed-service-r2/software-v2-native-execution-preparation-r3'
R3_SHA='c218a11aa4bd1307cf5e73f9c7fe76dce89af7e7e6da411f4d6b673377bb4b9b'
REFERENCE=REPO/'artifacts/advanced-split/models/weights-v2/software-v2-independent-numeric-r1'
METADATA_SELECTOR='arbiter::runtime::qualification::regeneration::emit_current_policy_profiles_from_verified_original_assets'
TEMPLATE_SELECTOR='inference_engine::pool::arbiter::qualification::software_v2::emit_software_v2_native_numeric_template'

def read(path):return json.loads(Path(path).read_bytes())
def digest(path):
 with Path(path).open('rb') as source:return hashlib.file_digest(source,'sha256').hexdigest()
def checked(pin):
 path=Path(pin['path']).resolve(strict=True)
 for parent in [path,*path.parents]:
  if parent.is_symlink() or getattr(parent.lstat(),'st_file_attributes',0)&0x400:raise ValueError('redirected input refused')
 if not path.is_file() or digest(path)!=pin['sha256']:raise ValueError('frozen input changed: '+str(path))
 return path
def file_pin(path):
 path=Path(path).resolve(strict=True);return {'path':str(path),'sha256':digest(path)}
def load(path,name):
 spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module

def baseline():
 preparation=read(checked({'path':str(R3/'preparation.json'),'sha256':R3_SHA}))
 launcher=load(checked({'path':str(R3/'launch_native.py'),'sha256':preparation['scripts'][str(R3/'launch_native.py')]}),'reviewed_native_bridge_launcher')
 summary=read(checked(preparation['source_summary']))
 manifest=read(checked(read(checked(summary['models'][0]['resource_run_input']))['source_manifest']))
 checked(preparation['reference_campaign'])
 if len(manifest['source_files'])!=15 or len(preparation['source_changes'])!=4:raise ValueError('expected original15 plus observer4 source closure')
 for pin in manifest['source_files']:checked(pin)
 sources=list(manifest['source_files'])
 mechanical=read(HERE/'line-ending-source-bridge.json')
 if mechanical['original_R3_preparation']['sha256']!=R3_SHA:raise ValueError('wrong mechanical source baseline')
 live_changes={}
 for row in mechanical['sources']:
  name=row['repository_path']
  if Path(row['live']['path'])!=REPO/name:raise ValueError('mechanical bridge redirected original source path')
  if preparation['source_changes'].get(name)!=row['proposal']['sha256']:raise ValueError('mechanical bridge substituted reviewed proposal')
  proposal=checked(row['proposal']).read_bytes();actual=checked(row['live']).read_bytes()
  if proposal.replace(b'\r\n',b'\n')!=actual.replace(b'\r\n',b'\n'):raise ValueError('source change exceeds explicit newline bridge')
  live_changes[name]=row['live']['sha256'];sources.append(row['live'])
 if set(live_changes)!=set(preparation['source_changes']):raise ValueError('mechanical bridge source roster differs')
 preparation=copy.deepcopy(preparation);preparation['source_changes']=live_changes
 preparation['mechanical_bridge']=file_pin(HERE/'line-ending-source-bridge.json')
 if len({pin['path'] for pin in sources})!=19:raise ValueError('distinct source closure required')
 return preparation,launcher,manifest,sources

def compare_request(before,fresh,profile):
 if set(before)!=set(fresh) or any(before[k]!=fresh[k] for k in before if k not in ['source_manifest','profile','files']):raise ValueError('native semantic request/policy changed')
 if set(before['files'])!=set(fresh['files']):raise ValueError('native logical asset roster changed')
 for asset in profile['assets']:
  if asset['role'] in ['model','external_weights','tokenizer'] and before['files'][asset['name']]!=fresh['files'][asset['name']]:raise ValueError('original reference-sealed neural/tokenizer path changed')

def write_new(path,value):
 with Path(path).open('x',encoding='utf-8') as output:json.dump(value,output,indent=2);output.write('\n')

def run_command(command,env,output,name):
 started=time.monotonic();result=subprocess.run(command,cwd=REPO,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 log=output/(name+'.log');log.write_bytes(result.stdout)
 receipt={'command':list(map(str,command)),'exit_code':result.returncode,'elapsed_seconds':time.monotonic()-started,'stdout':file_pin(log),'selected_model_forward_invocations':0}
 write_new(output/(name+'-execution.json'),receipt)
 if result.returncode:raise RuntimeError('NN0 command failed; original diagnostic retained: '+name)
 return receipt

def cargo_inputs(path,expected,images):
 raw=checked({'path':path,'sha256':expected}).read_text(encoding='utf-8')
 rows=[]
 for line in raw.splitlines():
  try:rows.append(json.loads(line))
  except json.JSONDecodeError:pass
 for image in images:
  matches=[row for row in rows if row.get('reason')=='compiler-artifact' and row.get('profile',{}).get('test') is True and row.get('executable')==image['original_path']]
  if len(matches)!=1:raise ValueError('actual exact Cargo compiler-artifact row missing/ambiguous')
 ort=[row for row in rows if row.get('reason')=='build-script-executed' and str(row.get('package_id','')).endswith('#ort-sys@2.0.0-rc.10') and row.get('linked_libs')==['static=onnxruntime']]
 if not ort or any(not row.get('linked_paths') or not all(isinstance(path,str) and path.startswith('native=') for path in row['linked_paths']) for row in ort):raise ValueError('actual recorded static ORT build script/path missing')
 return {'cargo_json':{'path':path,'sha256':expected},'actual_artifacts':[row for row in rows if row.get('reason')=='compiler-artifact' and any(row.get('executable')==image['original_path'] for image in images)],'actual_static_ort_records':ort}

def execute(args):
 preparation,launcher,original_manifest,sources=baseline()
 worker=load(checked({'path':str(REFERENCE/'reference_worker.py'),'sha256':'9c04be28ef06c0a06b8e2cffd4c274348ff141046114cc493d725ce527b694c7'}),'verified_human_identity')
 if worker.windows_sid()!='S-1-5-21-3678370870-3858642417-826968105-1002':raise ValueError('verified human execution required')
 packet_path=checked({'path':args.image_packet,'sha256':args.image_packet_sha256});packet=read(packet_path)
 oldcard=read(checked(next(card for card in preparation['cards'] if card['name']=='focus-resource')));oldcard['required_observer_source_changes']=dict(preparation['source_changes'])
 image,bridge=launcher.new_image(SimpleNamespace(image_packet=args.image_packet,image_packet_sha256=args.image_packet_sha256,source_bridge=args.source_bridge,source_bridge_sha256=args.source_bridge_sha256),oldcard)
 retrievals=[row for row in packet['images'] if row['binary_id']=='airk-retrieval']
 if len(retrievals)!=1:raise ValueError('actual preserved Retrieval image missing')
 retrieval=retrievals[0];checked(retrieval)
 cargo=cargo_inputs(args.cargo_json,args.cargo_json_sha256,[retrieval,image])
 build=read(checked(packet['actual_build_receipt']))
 for pin in sources:
  relative=Path(pin['path']).relative_to(REPO).as_posix()
  if build['groups']['native']['files'].get(relative)!=pin['sha256']:raise ValueError('declared source missing/mismatched in genuine build receipt: '+relative)
 output=Path(args.output).resolve()
 if output.exists() or not output.is_relative_to(PRIOR) or not output.parent.is_dir():raise ValueError('fresh immediate approved-parent descendant required')
 preflight={'reviewed_combined_bridge':bridge,'source_files':sources,'cargo':cargo,'R3_preparation':{'path':str(R3/'preparation.json'),'sha256':R3_SHA},'mechanical_source_bridge':preparation['mechanical_bridge'],'model_execution':False}
 env=dict(os.environ);env.update({'TEMP':'C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z/temp','TMP':'C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z/temp','TMPDIR':'C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z/temp','AIRK_PRIVATE_CENSUS_BASE':'C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z/private-census','PYTHONDONTWRITEBYTECODE':'1'})
 request=output/'metadata-request.private.json';metadata=output/'metadata'
 command=[sys.executable,'-B',str(REPO/'scripts/dev/prepare-arbiter-metadata.py'),'--repository-root',str(REPO),'--catalog',original_manifest['source_catalog']['path'],'--acquisition-receipt',original_manifest['acquisition_receipt']['path'],'--asset-census',original_manifest['asset_census']['path'],'--native-build-receipt',packet['actual_build_receipt']['path'],'--asset-root',original_manifest['asset_root'],'--request',str(request),'--output',str(metadata)]
 for pin in sources:command.extend(['--source-file',pin['path']])
 if args.audit_b2:
  print(json.dumps({'preflight':preflight,'metadata_request_command':command,'native_metadata_command':[retrieval['path'],'--ignored','--exact',METADATA_SELECTOR,'--nocapture','--test-threads=1'],'native_template_selector':TEMPLATE_SELECTOR,'proposed_fresh_output':str(output),'native_or_model_execution':False}));return
 output.mkdir();write_new(output/'source-bridge-preflight.json',preflight)
 run_command(command,env,output,'freeze-metadata-request')
 env.update({'AIRK_CONTEXT_METADATA_REGEN_INPUT':str(request),'AIRK_CONTEXT_METADATA_REGEN_INPUT_SHA256':digest(request)})
 run_command([retrieval['path'],'--ignored','--exact',METADATA_SELECTOR,'--nocapture','--test-threads=1'],env,output,'native-metadata')
 report=read(metadata/'metadata-regeneration-report.private.json');source_summary=read(checked(preparation['source_summary']));before=read(checked(source_summary['metadata_report']))
 if report['complete'] is not True or report['error'] is not None or report['NN_invocations']!=0 or len(report['profiles'])!=3 or report['native_binary_sha256']!=retrieval['sha256'] or report['native_build_receipt']!=packet['actual_build_receipt'] or report['native_source_group']!=packet['native_group']:raise ValueError('actual native metadata build/source closure differs')
 results=[]
 for prior in before['profiles']:
  row=next(row for row in report['profiles'] if row['model']==prior['model'])
  if checked(row['profile']).read_bytes()!=checked(prior['profile']).read_bytes():raise ValueError('Profile bytes changed; reference reuse refused')
  profile=read(row['profile']['path']);old_request=read(checked(prior['resource_run_input']));fresh=read(checked(row['resource_run_input']))
  compare_request(old_request,fresh,profile)
  directory=output/row['model'];directory.mkdir();template=directory/'blank-template.json'
  env.update({'AIRK_SOFTWARE_V2_NATIVE_INPUT':row['resource_run_input']['path'],'AIRK_SOFTWARE_V2_NATIVE_INPUT_SHA256':row['resource_run_input']['sha256'],'AIRK_SOFTWARE_V2_NATIVE_OUTPUT':str(template)})
  run_command([image['path'],'--ignored','--exact',TEMPLATE_SELECTOR,'--nocapture','--test-threads=1'],env,output,row['model']+'-template')
  old_template=checked(next(model for model in source_summary['models'] if model['model']==row['model'])['template'])
  if template.read_bytes()!=old_template.read_bytes():raise ValueError('complete native blank template changed; reuse refused')
  original=next(row for row in read(checked(preparation['reference_campaign']))['assemblies'] if read(checked(row['producer']))['model']==profile['model'])
  numeric=copy.deepcopy(fresh);numeric['numeric_fixture']=copy.deepcopy(original['fixture'])
  numeric_path=directory/'numeric-resource-run-input.private.json';write_new(numeric_path,numeric)
  checked(original['producer']);checked(original['fixture'])
  results.append({'model':profile['model'],'resource_input':row['resource_run_input'],'numeric_input':file_pin(numeric_path),'new_blank_template':file_pin(template),'old_blank_template':file_pin(old_template),'profile':row['profile'],'unchanged_profile_assets':profile['assets'],'immutable_reference_producer':original['producer'],'immutable_reference_fixture':original['fixture'],'original_reference_executions':original['reference_executions']})
 if len(results)!=3 or sum(len(row['unchanged_profile_assets']) for row in results)!=23:raise ValueError('full three-model23-asset bridge required')
 write_new(output/'bridge-result.json',{'complete':True,'source_bridge':bridge,'native_metadata_report':file_pin(metadata/'metadata-regeneration-report.private.json'),'models':results,'selected_model_forward_invocations':0,'qualification_granted':False,'mechanical_source_bridge':preparation['mechanical_bridge'],'pending':'Root review and R3 rebind_inputs.py using explicit mechanical source-pin refresh; actual native numeric/resource execution not performed.'})
 print(json.dumps({'bridge_result':file_pin(output/'bridge-result.json'),'NN_invocations':0,'qualification_granted':False}))

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--execute-nn0',action='store_true');p.add_argument('--audit-b2',action='store_true')
 for name in ['image-packet','image-packet-sha256','source-bridge','source-bridge-sha256','cargo-json','cargo-json-sha256','output']:p.add_argument('--'+name)
 a=p.parse_args()
 if not a.execute_nn0 and not a.audit_b2:
  _,_,_,sources=baseline();print(json.dumps({'status':'Preparation only; no emitted metadata, process or NN execution.','source_files':sources,'selectors':{'metadata':METADATA_SELECTOR,'blank_template':TEMPLATE_SELECTOR},'metadata_environment':['AIRK_CONTEXT_METADATA_REGEN_INPUT','AIRK_CONTEXT_METADATA_REGEN_INPUT_SHA256'],'template_environment':['AIRK_SOFTWARE_V2_NATIVE_INPUT','AIRK_SOFTWARE_V2_NATIVE_INPUT_SHA256','AIRK_SOFTWARE_V2_NATIVE_OUTPUT'],'required_actual_inputs':['B2 preserved image packet','Root-reviewed complete per-path source bridge','genuine Cargo artifact/staticORT JSON','fresh protected output'],'following_rebind_helper':str(R3/'rebind_inputs.py')}));return
 if any(getattr(a,name) is None for name in ['image_packet','image_packet_sha256','source_bridge','source_bridge_sha256','cargo_json','cargo_json_sha256','output']):p.error('actual independently pinned image/source/Cargo evidence and fresh output required')
 execute(a)
if __name__=='__main__':main()