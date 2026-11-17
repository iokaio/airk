# SPDX-License-Identifier: Apache-2.0
"""Preserve exact current CUDA-linked Inference/Retrieval lib-test artifacts only."""
from pathlib import Path
import argparse,datetime,hashlib,json,os,shutil,subprocess
R=Path('D:/code/Github/airk');E=R/'artifacts/v190/final-plan/exec-20261116T052942Z'
O=R/'artifacts/advanced-split/models/actual-composed-service-r2/final-plan-current-policy-preparation-20261116T085632Z/current-orchestration'
B=O/'gpu-first-fp32-approved-images-r1';RECEIPT=E/'gpu-first-fp32-approved-image-metadata-r1.json';LOG=E/'gpu-first-fp32-approved-image-metadata-r1.log'
C=Path('C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z');ENV=C/'gpu-checkpointed-static-environment-r1.json';ARCHIVE=C/'cu8/gpu-aggregate/onnxruntime.lib'
SID='S-1-5-21-3678370870-3858642417-826968105-1002';COMMIT='c02be86aed4b49b85b36cf92017097e6c930966f'
ENV_SHA='a348c538e349e8311841654ddce2365c229fb5972670f0b33cd0c71a3c882ef2';ARCHIVE_SHA='042c970193bf3d2fc7c80f53b055f74130c653d561f7735142fc01b01cebac00';ARCHIVE_BYTES=899807276
ARGV=['test','-p','airk-retrieval','-p','airk-inference','--lib','--all-features','--locked','--no-run','--message-format=json','--timings']
def require(ok,message):
 if not ok:raise ValueError(message)
def regular(path):
 p=Path(path);require(p.is_absolute() and p.resolve(strict=True)==p,'canonical absolute input required')
 for x in [p,*p.parents]:
  st=x.lstat();require(not x.is_symlink() and not getattr(st,'st_file_attributes',0)&0x400,'reparse/alias refused')
 require(p.is_file() and p.stat().st_nlink==1,'single-link regular file required');return p
def sha(p):
 with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def pin(p):p=regular(p);return {'path':str(p),'bytes':p.stat().st_size,'sha256':sha(p)}
def pe(p):
 with Path(p).open('rb') as f:
  h=f.read(64);require(len(h)==64 and h[:2]==b'MZ','PE DOS header required');offset=int.from_bytes(h[60:64],'little');require(64<=offset<=1<<20,'bounded PE offset');f.seek(offset);h=f.read(26);require(len(h)==26 and h[:4]==b'PE\0\0','PE signature required');machine=int.from_bytes(h[4:6],'little');magic=int.from_bytes(h[24:26],'little');require((machine,magic)==(0x8664,0x20b) and not int.from_bytes(h[22:24],'little')&0x2000,'PE64 x64 executable required');return {'machine_u16':machine,'optional_header_magic_u16':magic}
def policy(path):
 text="$a=Get-Acl -LiteralPath '"+str(path).replace("'","''")+"'; @{owner=$a.Owner;owner_sid=([Security.Principal.NTAccount]$a.Owner).Translate([Security.Principal.SecurityIdentifier]).Value;sddl=$a.Sddl;rules=@($a.Access|ForEach-Object{@{sid=$_.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value;rights=[int64]$_.FileSystemRights;type=$_.AccessControlType.ToString()}})}|ConvertTo-Json -Depth 5 -Compress"
 return json.loads(subprocess.check_output(['C:/Program Files/PowerShell/7/pwsh.exe','-NoProfile','-Command',text],text=True))
def private_owner(value):
 require(value['owner_sid']==SID,'current human file/directory owner required')
 require(not any(r['type']=='Allow' and r['sid']!=SID and r['rights']&0x500d0156 for r in value['rules']),'foreign writable preservation output refused')
def file_id(p):
 s=Path(p).stat();return {'volume':s.st_dev,'file_id':s.st_ino,'links':s.st_nlink,'bytes':s.st_size,'mtime_ns':s.st_mtime_ns}
def cargo_rows(text):
 rows=[json.loads(line) for line in text.splitlines() if line.lstrip().startswith('{')]
 finish=[r for r in rows if r.get('reason')=='build-finished'];require(len(finish)==1 and finish[0].get('success') is True,'exact one actual successful Cargo finish required')
 ort=[r for r in rows if r.get('reason')=='build-script-executed' and '#ort-sys@' in r.get('package_id','')]
 require(len(ort)==1 and ort[0]['package_id'].endswith('#ort-sys@2.0.0-rc.10'),'exact ort-sys rc10 row required')
 require(ort[0]['linked_libs']==['static=onnxruntime'] and ort[0]['linked_paths']==['native='+str(ARCHIVE.parent)],'actual exact CUDA static-core link required')
 selected=[]
 for package in ['airk-inference','airk-retrieval']:
  matches=[r for r in rows if r.get('reason')=='compiler-artifact' and r.get('profile',{}).get('test') is True and r.get('executable') and r.get('target',{}).get('kind')==['lib'] and r['target'].get('name')==package.replace('-','_') and Path(r.get('manifest_path','')).resolve()==(R/'crates'/package/'Cargo.toml').resolve()]
  require(len(matches)==1 and 'gpu-cuda' in matches[0].get('features',[]),'one actual GPU-featured lib-test artifact required: '+package);selected.append((package,matches[0]))
 return ort,selected

def write(p,v):
 with Path(p).open('x',encoding='utf-8') as f:json.dump(v,f,indent=2);f.write('\n')
def compile_context(path,expected):
 if path is None and expected is None:return None
 require(path is not None and expected is not None and len(expected)==64 and set(expected)<=set('0123456789abcdef'),'explicit compile-context path/SHA pair required')
 result=pin(path);require(result['bytes']<=16<<20 and result['sha256']==expected,'actual caller-pinned compile context differs')
 return result

def main():
 ap=argparse.ArgumentParser();ap.add_argument('--receipt-sha256',required=True);ap.add_argument('--native-group',required=True);ap.add_argument('--approved-script-sha256',required=True);ap.add_argument('--output',type=Path,required=True);ap.add_argument('--compile-input-context',type=Path);ap.add_argument('--compile-input-context-sha256');a=ap.parse_args()
 require(sha(__file__)==a.approved_script_sha256,'reviewed exact preserver source required')
 require(all(len(x)==64 and set(x)<=set('0123456789abcdef') for x in [a.receipt_sha256,a.native_group]),'explicit actual receipt/source-group SHAs required')
 who=subprocess.check_output(['C:/Windows/System32/whoami.exe','/user','/fo','csv','/nh'],text=True);logged=subprocess.check_output(['C:/Windows/System32/quser.exe'],text=True);require(SID in who and any('tdotn' in x.lower() and x.lstrip().startswith('>') for x in logged.splitlines()),'actual same-human SID/session required')
 require(subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip()==COMMIT,'exact current source commit required');require(a.output==B and B.is_absolute() and not B.exists() and B.parent.is_dir() and B.parent.resolve()==B.parent,'exact fresh preservation output required')
 parent_policy=policy(B.parent);private_owner(parent_policy)
 context=compile_context(a.compile_input_context,a.compile_input_context_sha256)
 rp=pin(RECEIPT);lp=pin(LOG);ep=pin(ENV);archive=pin(ARCHIVE)
 require(rp['sha256']==a.receipt_sha256 and ep['sha256']==ENV_SHA and archive['sha256']==ARCHIVE_SHA and archive['bytes']==ARCHIVE_BYTES,'actual build/static environment/archive pins differ')
 v=json.loads(RECEIPT.read_bytes());require(v.get('schema')=='airk-readiness-command/1' and v.get('status')=='completed' and v.get('passed') is True and v.get('exit_code')==v.get('runner_exit_code')==0,'actual completed successful native Cargo receipt required')
 require(all(v.get(k) is True for k in ['inputs_unchanged','runner_unchanged','runtime_unchanged','command_inputs_unchanged']) and v.get('changed_groups')==[] and v['groups']['native']['sha256']==a.native_group and v.get('incremental')=='0','unchanged actual native source/run scope required')
 require(v['log']==LOG.name and v['command'][1:]==ARGV,'exact reviewed Cargo no-run JSON command required')
 env=json.loads(ENV.read_bytes());require(env['schema']=='airk.cuda-static-environment/4' and env['activation_pins_available'] is False and env['aggregate']['output']['sha256']==ARCHIVE_SHA,'genuine unqualified checkpoint static handoff required')
 expected={'LIBONNXRUNTIME_NO_PKG_CONFIG':'1','ORT_SKIP_DOWNLOAD':'1','ORT_PREFER_DYNAMIC_LINK':'0','ORT_LIB_LOCATION':str(ARCHIVE.parent),'ORT_LIB_PROFILE':'Release'};require(env['environment']==expected,'exact five-value static CUDA environment required')
 ort,selected=cargo_rows(LOG.read_text(encoding='utf-8'));sources={};owners={}
 for package,row in selected:
  p=regular(row['executable']);require(p.resolve().is_relative_to(Path(v['target_directory']).resolve()) and 0<p.stat().st_size<=4<<30,'native artifact outside actual target/bounds');pe(p);owners[str(p)]=policy(p);require(owners[str(p)]['owner_sid']==SID,'foreign native image owner refused');sources[str(p)]=pin(p)
 def reopen():
  require(compile_context(a.compile_input_context,a.compile_input_context_sha256)==context,'caller compile-context drift')
  require(pin(RECEIPT)==rp and pin(LOG)==lp and pin(ENV)==ep and pin(ARCHIVE)==archive,'build/static input drift');require(policy(B.parent)==parent_policy,'preservation parent policy drift');require(subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip()==COMMIT,'source commit drift')
  for path,row in sources.items():require(pin(path)==row and policy(path)==owners[path],'original native bytes/owner/ACL drift')
 reopen();B.mkdir();private_owner(policy(B));images=B/'images';images.mkdir();private_owner(policy(images));result=[];bindings=[];outcome={'status':'RUNNING','source_commit':COMMIT,'native_group':a.native_group,'native_model_request_dispatched':False}
 try:
  for package,row in selected:
   original=regular(row['executable']);before=file_id(original);source_pin=sources[str(original)];target=images/original.name
   with original.open('rb') as src,target.open('xb') as dst:shutil.copyfileobj(src,dst,1<<20)
   target_pin=pin(target);require((target_pin['bytes'],target_pin['sha256'])==(source_pin['bytes'],source_pin['sha256']) and pin(original)==source_pin and file_id(original)==before,'actual independent copy/source stability differs');require((target.stat().st_dev,target.stat().st_ino)!=(original.stat().st_dev,original.stat().st_ino),'independent file IDs required');target_policy=policy(target);private_owner(target_policy);require(policy(original)==owners[str(original)],'original ACL changed');machine=pe(target)
   image={'binary_id':package,'path':str(target),'bytes':target_pin['bytes'],'sha256':target_pin['sha256'],'original_path':str(original),'original_file_owner_sid':SID,'preserved_file_owner_sid':SID};result.append(image);bindings.append({'materialized_image':image,'actual_cargo_artifact':row,'pe':machine,'original_owner_policy':owners[str(original)],'preserved_owner_policy':target_policy,'source_file_identity_before':before,'source_file_identity_after':file_id(original),'preserved_file_identity':file_id(target),'source_stat_unchanged':True,'independent_file_ids':True,'source_and_dest_nlink1':True})
  reopen()
  inventory={'schema':'airk.preserved-native-test-images/1','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source_commit':COMMIT,'native_group':a.native_group,'actual_build_receipt':{'path':str(RECEIPT),'sha256':rp['sha256']},'images':result,'source_dirty':bool(subprocess.check_output(['git','status','--porcelain','--untracked-files=no'],cwd=R,text=True)),'reviewed_delta':'Actual current CUDA-linked no-run CargoJSON; original failed builds and CPU evidence remain historical. No qualification transferred.','scope':'Exact two GPU-featured lib-test PE64 artifacts and independent byte copies only; no testcase, GPU, model, signing, installation or activation run.'}
  name=B/'native-test-images-first-fp32-approved-r1.json';write(name,inventory);write(B/'source-owners.json',[{'path':path,**value} for path,value in owners.items()]);write(B/'cargo-image-bindings.json',{'schema':'airk.private-current-image-preservation-binding/1','inventory':pin(name),'build_receipt':rp,'cargo_json':lp,'actual_static_ORT_rows':ort,'cuda_static_environment':ep,'actual_static_archive':archive,'caller_compile_input_context':context,'images':bindings,'preserver_source':pin(Path(__file__).resolve()),'creation_policy':'Exact fresh private human-owned parent inheritance; no existing owner/ACL changes.','testcases_listed_or_run':False,'native_model_request_dispatched':False,'artifact_or_target_approval':'pending explicit Root review; none invented'});outcome.update(status='PASS_IMAGE_PRESERVATION_ONLY_UNQUALIFIED',inventory=pin(name))
 except BaseException as exc:outcome.update(status='FAILED_PRESERVED_NO_RETRY',error=type(exc).__name__+': '+str(exc));raise
 finally:write(B/'preservation-result.json',outcome)
 print(json.dumps(outcome))
if __name__=='__main__':main()
