# SPDX-License-Identifier: Apache-2.0
"""Preserve actual R95 image/target evidence and explicitly scoped historical R92 provenance."""
import hashlib,json,os,struct,subprocess,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[5]
HERE=Path(__file__).resolve().parent
TARGET='x86_64-pc-windows-msvc'
OLD='c721302898f023e347498ef57d16ab882dac565abfcc5018154344bd6c260e69'
NEW='6545c592ed234f8ea1338fa99651bef880fd9aa984e61f48deab78df48b960f9'
EXPECTED_SOURCE='64a0a51d6893b54b23e55706be7aaa0c5e05382fefecff2e3f984479dd26c8d0'
CHUNK=4<<20

def sha(path):
 with Path(path).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def pe(path):
 with Path(path).open('rb') as f:
  head=f.read(64)
  if len(head)!=64 or head[:2]!=b'MZ':raise ValueError('DOS image signature differs')
  offset=struct.unpack_from('<I',head,60)[0];f.seek(offset);header=f.read(26)
  if header[:4]!=b'PE\0\0':raise ValueError('PE signature differs')
  machine=struct.unpack_from('<H',header,4)[0];magic=struct.unpack_from('<H',header,24)[0]
  if machine!=0x8664 or magic!=0x20b:raise ValueError('Actual current image is not AMD64 PE32+')
  return {'machine_u16':machine,'machine_hex':'0x8664','optional_header_magic_u16':magic,'optional_header_magic_hex':'0x020b','signature':'PE','recomputed_from_actual_preserved_bytes':True}
def cargo_artifact(path,name):
 matches=[]
 for line in path.read_text(encoding='utf-8').splitlines():
  try:row=json.loads(line)
  except json.JSONDecodeError:continue
  if row.get('reason')=='compiler-artifact' and row.get('target',{}).get('name')==name and row.get('profile',{}).get('test') is True and row.get('executable'):matches.append(row)
 if len(matches)!=1:raise ValueError('Unique actual Cargo test image record required')
 return matches[0]
def main():
 started=time.monotonic();out=HERE/'build-target-evidence-r1'
 if out.exists():raise ValueError('Fresh immutable evidence output required')
 out.mkdir();sources=[]
 def snapshot(source,name):
  source=Path(source).resolve();destination=out/name
  if destination.exists() or source.is_symlink() or not source.is_file():raise ValueError('Fresh regular-file snapshot required')
  before=sha(source);count=0;h=hashlib.sha256()
  with source.open('rb') as f,destination.open('xb') as target:
   while block:=f.read(CHUNK):target.write(block);count+=len(block);h.update(block)
  if h.hexdigest()!=before or sha(source)!=before or sha(destination)!=before or count!=source.stat().st_size:raise ValueError('Source changed or materialized copy differs')
  value={'path':str(destination),'sha256':before,'bytes':count,'original_path':str(source),'independent_copy':True};sources.append(value);return value
 def read_snapshot(source,name):
  value=snapshot(source,name);return value,json.loads(Path(value['path']).read_bytes())
 current=[]
 for crate,filename in [('airk_inference','airk-inference-image'),('airk_kernel','airk-kernel-image')]:
  rdir=ROOT/'artifacts/advanced-split/receipts/stage-integration-r95';buildpin,build=read_snapshot(rdir/(filename+'.json'),'r95-'+filename+'-receipt.json');logpin=snapshot(rdir/(filename+'.log'),'r95-'+filename+'-cargo.jsonl');row=cargo_artifact(Path(logpin['path']),crate)
  if build['exit_code']!=0 or build['inputs_unchanged'] is not True or build['groups']['native']['sha256']!=EXPECTED_SOURCE or '--target' in build['command'] or any(str(x).startswith('--target=') for x in build['command']):raise ValueError('Actual successful unchanged R95 host-build receipt required')
  binary=Path(row['executable']);expected_parent=Path(build['target_directory'])/'debug/deps'
  if binary.parent!=expected_parent:raise ValueError('Actual Cargo output is not retained host/default-target directory')
  materialized=snapshot(binary,crate+'-r95.exe')
  if crate=='airk_inference' and materialized['sha256']!=NEW:raise ValueError('Actual Root-reviewed R95 inference identity differs')
  current.append({'image_sha256':materialized['sha256'],'target':TARGET,'artifact_kind':crate,'evidence_kind':'materialized_current_PE_and_recorded_build','materialized_image':materialized,'pe':pe(Path(materialized['path'])),'cargo_json':logpin,'cargo_selector':{'reason':'compiler-artifact','target_name':crate,'profile_test':True,'executable':row['executable']},'actual_cargo_artifact':row,'build_receipt':buildpin,'native_source_group':EXPECTED_SOURCE,'build_command_has_explicit_target':False,'actual_host_output_directory':str(expected_parent)})
 r92=ROOT/'artifacts/advanced-split/receipts/stage-integration-r92';oldpin,old=read_snapshot(r92/'inference-image-identity.json','r92-inference-image-identity.json');oldbuildpin,oldbuild=read_snapshot(r92/'inference-image.json','r92-inference-build-receipt.json');oldlogpin=snapshot(r92/'inference-image.log','r92-inference-cargo.jsonl');oldrow=cargo_artifact(Path(oldlogpin['path']),'airk_inference')
 if old['sha256']!=OLD or old['actual_cargo_artifact']!=oldrow or old['source_receipt_sha256']!=oldbuildpin['sha256'] or oldbuild['exit_code']!=0 or not oldbuild['inputs_unchanged'] or old['source_group']!=oldbuild['groups']['native']['sha256'] or '--target' in oldbuild['command'] or any(str(x).startswith('--target=') for x in oldbuild['command']):raise ValueError('Exact original successful R92 identity/build closure required')
 proofs=[];focus=ROOT/'artifacts/advanced-split/focus-native-qualification-preparation-r1'
 for stage in ['native-parity-r2','native-resource-r2']:
  executionpin,execution=read_snapshot(focus/stage/'execution.json','focus-'+stage+'-execution.json');requestpin,request=read_snapshot(focus/stage/'request.json','focus-'+stage+'-request.json');owned=execution['owned'];image=owned['suspended_assignment']['image']
  if execution['complete'] is not True or execution['post_input_drift'] or image['sha256']!=OLD or request['pins'].get(image['path'])!=OLD or request['command'][0]!=image['path'] or owned['exit_code']!=0 or owned['owned_job_final_active_processes']!=0 or owned['owned_job_final_member_pids'] or owned['owned_job_forced_termination']:raise ValueError('Original natural Windows job image proof differs')
  proofs.append({'execution':executionpin,'request':requestpin,'image_json_pointer':'/owned/suspended_assignment/image','captured_image':image,'actual_Windows_owned_process_image_hash_proven':True})
 rustc_command=['C:/Users/tdotn/.cargo/bin/rustc.exe','-vV'];probe=subprocess.run(rustc_command,capture_output=True,text=True,check=True,timeout=15)
 if 'host: '+TARGET not in probe.stdout:raise ValueError('Actual rustc host differs')
 (out/'rustc-vV.txt').write_text(probe.stdout,encoding='utf-8');rustcpin={'path':str(out/'rustc-vV.txt'),'sha256':sha(out/'rustc-vV.txt'),'bytes':(out/'rustc-vV.txt').stat().st_size,'command':rustc_command,'exit_code':probe.returncode}
 cachedpin,cached=read_snapshot(ROOT/'artifacts/v190/p10-target/.rustc_info.json','target-rustc-info.json');cached_outputs=[r.get('stdout','') for r in cached['outputs'].values() if r.get('success') is True]
 if not any('host: '+TARGET in x for x in cached_outputs) or not any(all(token in x for token in ['target_env="msvc"','target_arch="x86_64"','target_os="windows"']) for x in cached_outputs):raise ValueError('Retained target cache does not corroborate exact Windows/MSVC target')
 static=ROOT/'artifacts/advanced-split/ort130-x64-owned-r5';staticpin,staticbuild=read_snapshot(static/'build-receipt.json','ort-r5-build-receipt.json');aggregatepin,aggregate=read_snapshot(static/'static-aggregate-receipt.json','ort-r5-static-aggregate-receipt.json');envpin,env=read_snapshot(static/'cargo-aggregate-env.json','ort-r5-cargo-env.json')
 if staticbuild['target']!=TARGET or env['environment']['ORT_PREFER_DYNAMIC_LINK']!='0' or env['environment']['ORT_SKIP_DOWNLOAD']!='1' or env['static_aggregate']['receipt']['output']!=aggregate['output']:raise ValueError('Original exact static ORT target/aggregate link records differ')
 configs=[snapshot(ROOT/path,name) for path,name in [('artifacts/advanced-split/run-stage-r95.py','root-stage-r95-runner.py'),('artifacts/advanced-split/run-stage-r92.py','root-stage-r92-runner.py'),('.cargo/config.toml','cargo-config.toml'),('rust-toolchain.toml','rust-toolchain.toml')]]
 target_basis={'rustc_probe':rustcpin,'cached_rustc_target':cachedpin,'static_ORT_target':staticbuild['target'],'static_ORT_build_receipt':staticpin,'static_ORT_aggregate_receipt':aggregatepin,'static_ORT_cargo_env':envpin,'static_library_recorded_identity':aggregate['output'],'static_library_rehashed_or_copied_in_this_stage':False,'build_configuration_records':configs,'no_explicit_cross_target_argument_in_retained_R92_R95_commands':True,'historical_ambient_CARGO_BUILD_TARGET_was_not_recorded':True,'scope':'Exact host/default target is corroborated by Cargo output layout, recorded rustc cached cfg, MSVC static ORT ABI, and real Windows job execution. PE machine alone does not distinguish MSVC from GNU.'}
 historical={'image_sha256':OLD,'target':TARGET,'artifact_kind':'airk_inference_focus_R92','evidence_kind':'historical_recorded_build_static_ABI_and_Windows_owned_image','materialized_image':None,'pe':None,'historical_image_bytes_overwritten':True,'original_image_identity':oldpin,'original_build_receipt':oldbuildpin,'original_cargo_json':oldlogpin,'actual_cargo_artifact':oldrow,'native_source_group':old['source_group'],'owned_Windows_image_proofs':proofs,'scope':'No old PE bytes, current PE parse or newly computed old-image hash is claimed. Original captured image hash and original build records are preserved under their historical identities.'}
 result={'schema':'airk.private-native-build-target-evidence/1','complete':True,'target':TARGET,'images':[*current,historical],'target_basis':target_basis,'source_snapshots':sources,'captured_seconds':time.monotonic()-started,'script_sha256':sha(__file__),'NN_executed':False,'Cargo_executed':False,'scope':'Build-target and preserved-image provenance only. No additional native numeric, semantic, composed, platform-release or publication qualification.'}
 receipt=out/'target-evidence.json';receipt.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
 for path in out.iterdir():
  if path.is_file():os.chmod(path,0o444)
 print(json.dumps({'receipt':str(receipt),'sha256':sha(receipt),'images':[{'image_sha256':x['image_sha256'],'target':x['target'],'evidence_kind':x['evidence_kind']} for x in result['images']],'preserved_current_image_bytes':sum(x['materialized_image']['bytes'] for x in current),'elapsed_seconds':result['captured_seconds']}))
if __name__=='__main__':main()
