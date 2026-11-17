# SPDX-License-Identifier: Apache-2.0
"""Minimal opaque current-source rebind only; no native launch or regeneration."""
from pathlib import Path
import argparse,copy,hashlib,json,os
P=Path(__file__).resolve().parent.parent
OLD=P/'compact-capacity-rebind-r2/resource-run-input-original-only.private.json'
OLD_SHA='5a083f82b180cfb5ba56bb3976c882df4f22e712e65677b9a841d039e31728a7'
OLD_GROUP='a469d6e189e70bd8abe41c07ea2d548cd6141145e33eea3f6e958c731ca12fdf'
NEW_GROUP='6ad5bb91a03111541424d47438e25b9d0895ff46cc32e912ea78528d949bb91b'
SCOPES={
 'crates/airk-retrieval/src/arbiter/runtime.rs':'Root-reviewed profiler solecaller now production common constructor+EnableProfiling only; no optimized-save request. Actual ordinaryopen/common runtime/tiny helper static byte-preservation proof retained; no math/input/weights/budget change.',
 'crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/attention_diagnostic.rs':'Root-reviewed diagnostic measurement /2 with optimized_save.enabled false and no optimized output claim; actual28CPU/3allocated outputs, original numeric/memory/deadline gates unchanged.',
 'crates/airk-inference/src/inference_engine/pool/arbiter/CONTRACT.md':'Root-reviewed private profiler /2 constructor/retention/measurement contract; no productionOOM inference from prior save-heavy refusals.',
 'crates/airk-installer/src/legacy_processes.rs':'Root-committed Mac legacy process path-miss ESRCH classification using fresh native BSD PID/UID/zombie observation at f5f9; independent from neural scoring. Exact whole-native source delta retained.',
 'crates/airk-installer/src/legacy_processes/CONTRACT.md':'Root-committed corresponding native Mac path/UID/exit-observation contract; no neural profile/math/data change.'}

def need(ok,message):
 if not ok:raise ValueError(message)
def pin(path):
 path=Path(path).absolute();need(path.is_file() and not path.is_symlink(),'Regular retained input required')
 with path.open('rb') as f:h=hashlib.file_digest(f,'sha256').hexdigest()
 return {'path':str(path),'sha256':h}
def read(value):
 need(pin(value['path'])==value,'Pinned bytes changed: '+value['path']);return json.loads(Path(value['path']).read_bytes())
def retain(path,value):
 with path.open('x',encoding='utf-8') as f:json.dump(value,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--images');p.add_argument('--images-sha256');p.add_argument('--stage',action='store_true');a=p.parse_args()
 if not a.stage:
  print(json.dumps({'stage_performed':False,'required':['actual successful current image inventory path','external inventory SHA256','--stage after Root review'],'old_group':OLD_GROUP,'new_group':NEW_GROUP,'exact_expected_changed_paths':list(SCOPES),'regeneration_or_model':False}));return
 need(a.images and a.images_sha256,'Actual successful image inventory pins required, no fallback')
 inventory={'path':str(Path(a.images).absolute()),'sha256':a.images_sha256};packet=read(inventory)
 need(packet['schema']=='airk.preserved-native-test-images/1' and packet['native_group']==NEW_GROUP,'Wrong actual current source image packet')
 build=read(packet['actual_build_receipt'])
 need(build['passed'] is True and build['exit_code']==0 and build['inputs_unchanged'] is True and build['groups']['native']['sha256']==NEW_GROUP,'Current actual build failed/not final')
 old_input={'path':str(OLD),'sha256':OLD_SHA};request=read(old_input);old_manifest_pin=request['source_manifest'];manifest=read(old_manifest_pin);oldbuild=read(manifest['native_build_receipt'])
 need(oldbuild['groups']['native']['sha256']==OLD_GROUP and manifest['native_build_receipt']['sha256']=='b1716e23cfdb2caa9125cf42f27361ef149037abcc7c2dddfc09f305dd1aaa1f','Actual old capacity build differs')
 prior=oldbuild['groups']['native']['files'];current=build['groups']['native']['files'];changes={k for k in set(prior)|set(current) if prior.get(k)!=current.get(k)}
 need(changes==set(SCOPES),'Actual whole-native delta is not exact approved profiling3+Mac2: '+str(sorted(changes)))
 repo=Path(manifest['repository_root']);sources=[]
 for row in manifest['source_files']:
  new=pin(row['path']);relative=Path(row['path']).relative_to(repo).as_posix()
  need(current.get(relative)==new['sha256'],'Source differs from actual new build seal: '+relative)
  if relative not in changes:need(new==row,'Unrelated original source drift')
  sources.append(new)
 image=next(x for x in packet['images'] if x['binary_id']=='airk-inference');need(pin(image['path'])['sha256']==image['sha256'],'Actual current inference image differs')
 newmanifest=copy.deepcopy(manifest);newmanifest.update(native_build_receipt=packet['actual_build_receipt'],source_commit=packet['source_commit'],source_files=sources)
 check=copy.deepcopy(newmanifest)
 for key in ['native_build_receipt','source_commit','source_files']:check[key]=manifest[key]
 need(check==manifest,'Unrelated opaque sourceManifest field changed')
 output=P/'compact-profile-only-rebind-r3';need(not output.exists(),'Fresh rebind output required');output.mkdir()
 manifestpath=output/'current-source-manifest.private.json';retain(manifestpath,newmanifest)
 newrequest=copy.deepcopy(request);newrequest['source_manifest']=pin(manifestpath);check=copy.deepcopy(newrequest);check['source_manifest']=request['source_manifest'];need(check==request,'Unrelated native Core request field changed')
 requestpath=output/'resource-run-input-original-only.private.json';retain(requestpath,newrequest)
 bridge={'schema':'airk.private.reviewed-native-source-bridge/1','status':'Prepared actual per-path bridge for Root final execution review; no new approval invented','old_native_group':OLD_GROUP,'new_native_group':NEW_GROUP,
  'old_build_receipt':manifest['native_build_receipt'],'new_build_receipt':packet['actual_build_receipt'],'changes':{k:{'before_sha256':prior.get(k),'after_sha256':current.get(k),'reviewed_scope':SCOPES[k]} for k in sorted(changes)}}
 bridgepath=output/'exact-five-source-bridge.json';retain(bridgepath,bridge)
 report={'schema':'airk.private.profile-only-rebind/3','native_input':pin(requestpath),'current_source_manifest':pin(manifestpath),'source_bridge':pin(bridgepath),'image_inventory':inventory,'current_image':image,
  'original_native_input':old_input,'original_metadata_generation_preserved':True,'Profile_compiler_template_fixture_graph_arrays_unchanged':True,'metadata_emission_repeated':False,'native_execution':False,'source_count':len(sources),'required_diagnostic_identity':'compact-fp32-gqa-profiled/2'}
 retain(output/'preparation.json',report);print(json.dumps(report))
if __name__=='__main__':main()