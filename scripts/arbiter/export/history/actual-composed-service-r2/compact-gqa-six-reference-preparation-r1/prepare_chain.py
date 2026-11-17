# SPDX-License-Identifier: Apache-2.0
"""Opaque copies of existing roster/owned plans only. No process/GPU/native launch."""
from pathlib import Path
import argparse,copy,hashlib,json,os
HERE=Path(__file__).resolve().parent
P=HERE.parent
REF=P.parent/'weights-v2/software-v2-independent-numeric-r1'
SOURCE_INPUT=P/'final-plan-current-policy-preparation-20261116T085632Z/nn0-native-preparation-r1/arbiter-context-qwen3-17b/original-source-input.json'
SOURCE_SHA='488ef2cde44d83abf7d2f167ad2c5af486da2bb7449c5047818fc4add5fbd8c0'
TEMPLATE=P/'compact-gqa-native-metadata-r1/template-emission-r1/blank-template.json'
TEMPLATE_SHA='7d4a546d9017a14594d61a59eda9693a47e241da4a5dd9ef1edf4bbb0e626c4b'
PARENT=HERE/'source/reference_parent_no_site_r2.py'
WORKER=HERE/'source/reference_worker.py'

def need(ok,message):
 if not ok:raise ValueError(message)
def pin(path):
 path=Path(path).resolve(strict=True)
 need(path.is_file() and not path.is_symlink(),'Regular original file required')
 with path.open('rb') as f:h=hashlib.file_digest(f,'sha256').hexdigest()
 return {'path':str(path),'bytes':path.stat().st_size,'sha256':h}
def corepin(path):return {k:v for k,v in pin(path).items() if k!='bytes'}
def checked(value):
 actual=pin(value['path']);need(actual['sha256']==value['sha256'] and ('bytes' not in value or actual['bytes']==value['bytes']),'Input changed: '+value['path']);return json.loads(Path(value['path']).read_bytes())
def retain(path,value):
 with path.open('x',encoding='utf-8') as f:json.dump(value,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def frozen_sources():
 need(pin(PARENT)['sha256']=='fe4cc990649d92e041b79cccb3fd7ef1129979a26432b7f02b2d8d28b586ed8d','Parent logic changed')
 need(pin(WORKER)['sha256']=='9cbca2db8d9cbbedd53e546d49bcd3ddb173a35f5583bc2d4681fa52eaa906a7','Reviewed worker changed')
 need(pin(TEMPLATE)['sha256']==TEMPLATE_SHA and pin(SOURCE_INPUT)['sha256']==SOURCE_SHA,'Native blank/publisher source input changed')
def roster(args):
 frozen_sources();packet=checked({'path':str(Path(args.images).resolve()),'sha256':args.images_sha256});image=next(x for x in packet['images'] if x['binary_id']=='airk-inference');need(pin(image['path'])['sha256']==image['sha256'],'Current image changed')
 original=checked({'path':str(Path(args.native_input).resolve()),'sha256':args.native_input_sha256});manifest=checked(original['source_manifest']);build=checked(manifest['native_build_receipt'])
 need(packet['actual_build_receipt']==manifest['native_build_receipt'] and packet['native_group']==build['groups']['native']['sha256'] and build['passed'] is True,'Native input must already bind actual new successful build/image')
 profile=checked(original['profile']);need(profile['model']=='arbiter-context-qwen3-17b' and next(a for a in profile['assets'] if a['role']=='model')['sha256']=='d2b32177c875a989bdbe56071168bbcabb8d589ba740099feea44972459828c3','Compact exact graph required')
 stage=HERE/'roster-stage';need(not stage.exists(),'Fresh roster stage required');stage.mkdir()
 request=copy.deepcopy(original);request['numeric_fixture']=None
 restored=copy.deepcopy(request);restored['numeric_fixture']=original['numeric_fixture'];need(restored==original,'Opaque request changed beyond absent expected-score fixture')
 requestpath=stage/'reference-native-input.private.json';retain(requestpath,request);output=stage/'original-roster.json'
 report={'schema':'airk.private.compact-fresh-reference-roster-stage/1','reference_native_input':pin(requestpath),'original_current_input':{'path':str(Path(args.native_input).resolve()),'sha256':args.native_input_sha256},'image_inventory':{'path':str(Path(args.images).resolve()),'sha256':args.images_sha256},'native_image':image,'native_roster_output':str(output),
 'command':[image['path'],'--exact','inference_engine::pool::arbiter::qualification::software_v2::original_sources::emit_software_v2_original_reference_assets','--ignored','--nocapture'],
 'environment':{'AIRK_SOFTWARE_V2_NATIVE_INPUT':str(requestpath),'AIRK_SOFTWARE_V2_NATIVE_INPUT_SHA256':pin(requestpath)['sha256'],'AIRK_SOFTWARE_V2_ORIGINAL_INPUT':str(SOURCE_INPUT),'AIRK_SOFTWARE_V2_ORIGINAL_INPUT_SHA256':SOURCE_SHA,'AIRK_SOFTWARE_V2_NATIVE_OUTPUT':str(output)},'native_launched':False,'NN_invocations':0}
 retain(stage/'stage.json',report);print(json.dumps(report))
def plans(args):
 frozen_sources();stage=checked(corepin(HERE/'roster-stage/stage.json'));native_pin=stage['reference_native_input'];request=checked(native_pin);template=checked({'path':str(TEMPLATE),'sha256':TEMPLATE_SHA});roster_pin={'path':str(Path(args.roster).resolve()),'sha256':args.roster_sha256};emitted=checked(roster_pin)
 need(emitted['native_input_sha256']==native_pin['sha256'] and emitted['original_source_input']==corepin(SOURCE_INPUT) and emitted['native_airk_nn_invocations']==0,'Actual fresh native roster does not bind this request')
 old_roster=checked(checked(corepin(REF/'plans-native-r2/compact-original_fp32.json'))['original_roster'])
 for key in ['original_reference_assets','original_reference_files','original_source_files','publisher_model','publisher_revision','publisher_manifest_sha256','compiler','model','resource_policy_digest']:need(emitted[key]==old_roster[key],'Original publisher identity changed: '+key)
 manifest=checked(request['source_manifest']);source_pins=[manifest['native_build_receipt'],*manifest['source_files']]
 for v in source_pins:need(pin(v['path'])['sha256']==v['sha256'],'Current native source changed')
 output=HERE/'source/plans';need(not output.exists(),'Fresh owned plan outputs required');output.mkdir();reports=[]
 for kind in ['original_fp32','same_artifact']:
  old=checked(corepin(REF/('plans-native-r2/compact-'+kind+'.json')));plan=copy.deepcopy(old)
  need(plan['limits']=={'wall_seconds':600,'host_committed_bytes':51539607552,'cuda_allocator_bytes':21474836480,'minimum_cuda_free_bytes':23622320128,'threads':2},'Original reference limits changed')
  plan.update(native_input=copy.deepcopy(native_pin),native_template=pin(TEMPLATE),original_roster=pin(roster_pin['path']),worker=pin(WORKER))
  # Retain old evidence pins. Refresh ONLY current native-source paths sealed
  # by the new actual build; all unrelated runtime/original-source seals stay.
  for v in source_pins:plan['input_seals'][str(Path(v['path']).resolve())]=v['sha256']
  required=[plan['native_input'],plan['native_template'],plan['original_roster'],plan['worker'],pin(PARENT),request['profile'],request['source_manifest'],emitted['original_source_input']]
  profile=checked(request['profile'])
  required += [dict(path=request['files'][a['name']],sha256=a['sha256'],bytes=a['bytes']) for a in profile['assets']]
  if kind=='same_artifact':
   plan['compact_gqa_binding']={'source_graph':pin(P/'final-plan-acquired-assets-20261116T071657Z/04b142e12147bb87e57a8bef98f1094aaafe38422b84cd0571be2fd842dd298d/model.onnx'),'shape_proof':pin(P/'compact-gqa-graph-r3/model.proof.json')};required+=list(plan['compact_gqa_binding'].values())
  for v in required:need(pin(v['path'])['sha256']==v['sha256'],'Exact current phase pin changed');plan['input_seals'][str(Path(v['path']).resolve())]=v['sha256']
  for path,expected in plan['input_seals'].items():need(pin(path)['sha256']==expected,'Retained unrelated source/runtime drift: '+path)
  path=output/('compact-'+kind+'.json');retain(path,plan);destination=HERE/'source'/('runs-compact-'+kind+'-r1');need(not destination.exists(),'Fresh actual run output required')
  reports.append({'plan':pin(path),'kind':kind,'limits':plan['limits'],'command':[plan['base_python']['path'],'-S','-B',str(PARENT),'--plan',str(path),'--plan-sha256',pin(path)['sha256'],'--output',str(destination),'--execute'],'expected_actual_forwards':3,'execution_performed':False})
 retain(output/'commands.json',reports);print(json.dumps(reports))
def main():
 p=argparse.ArgumentParser(description=__doc__);sub=p.add_subparsers(dest='mode');a=sub.add_parser('roster');a.add_argument('--native-input',required=True);a.add_argument('--native-input-sha256',required=True);a.add_argument('--images',required=True);a.add_argument('--images-sha256',required=True);a=sub.add_parser('plans');a.add_argument('--roster',required=True);a.add_argument('--roster-sha256',required=True);args=p.parse_args()
 if args.mode=='roster':roster(args)
 elif args.mode=='plans':plans(args)
 else:print(json.dumps({'prepared_only':True,'steps':['actual current sourceManifest/image native input required','roster files-only stage prints existingRustNN0command','Root actualNN0roster','plans files-only stage copies existing plans/source pair','Root review then3original+3sameartifact actualTorch forwards sequential','existingassembler; no arrays/approvals invented'],'NN':False}))
if __name__=='__main__':main()