# SPDX-License-Identifier: Apache-2.0
import os,pwd,pathlib,json,subprocess,time,hashlib,importlib.util
assert os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=='tyler';os.umask(0o077)
B=pathlib.Path('/home/tyler/airk-builds/arbiter-final-linux-r1');S=B/'source';assert B.resolve()==B and B.stat().st_uid==1000
W=pathlib.Path('/mnt/d/code/Github/airk');manifest=json.loads((B/'source-transfer/source-manifest.json').read_text());files=manifest['files'];record={'schema':'airk.private.linux-source-and-native-inputs/1','uid':os.getuid(),'source_passed':False,'models_passed':False,'static_archive_stage_passed':False,'commands':[],'static_archives':[],'models':[],'failures':[]};out=B/'evidence/source-and-native-inputs-r3.json';began=time.monotonic()
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def save():out.write_text(json.dumps(record,indent=2)+'\n')
def load(name,path):
 spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
def run(argv):
 p=subprocess.run(argv,capture_output=True,text=True,timeout=60);record['commands'].append({'argv':argv,'exit_code':p.returncode,'stdout':p.stdout,'stderr':p.stderr});save();assert p.returncode==0,record['commands'][-1];return p.stdout.strip()
def copy(original,target,size,digest):
 assert original.is_file() and not original.is_symlink() and not target.exists();target.parent.mkdir(parents=True,exist_ok=True);h=hashlib.sha256();count=0
 with original.open('rb') as src,target.open('xb') as dst:
  while True:
   data=src.read(1024*1024)
   if not data:break
   count+=len(data);assert count<=size;h.update(data);dst.write(data)
 assert count==size and h.hexdigest()==digest and sha(target)==digest and target.stat().st_uid==1000 and target.stat().st_nlink==1
 return {'source':str(original),'path':str(target),'bytes':count,'sha256':digest}
try:
 # Refresh real normalized index under tracked attributes; no source or commit rewrite.
 run(['/usr/bin/git','-C',str(S),'add','--renormalize','--all'])
 tree=run(['/usr/bin/git','-C',str(S),'write-tree']);assert tree==manifest['normalized_tree']
 status=run(['/usr/bin/git','-C',str(S),'status','--porcelain','--untracked-files=all']);assert not status
 head=run(['/usr/bin/git','-C',str(S),'rev-parse','HEAD']);assert head==manifest['commit']
 for row in files:
  p=S/row['path'];assert p.lstat().st_uid==1000 and p.stat().st_size==row['bytes'] and sha(p)==row['sha256'] and bool(p.stat().st_mode&0o111)==(row['mode']=='100755'),row['path']
 builder=load('linux_frozen_c7_owner',S/'scripts/build-service-runtime.py');c7=builder.source_identity();assert c7==manifest['raw_c7']=={'sha256':'06c740ee904ba9ccc3aec6a8359bff2b6e1f76bd061aa26e41e3c55a2bfd910e','files':1944}
 record.update(source_passed=True,head=head,tree=tree,git_clean=True,all3511_raw_hashes_modes_match=True,raw_c7=c7,git_metadata_note='Genuine clone normalized index refreshed by git add --renormalize; resulting tree exactly original HEAD; raw source untouched; no commit/trust/config override')
except BaseException as e:record['failures'].append({'stage':'source','type':type(e).__name__,'message':str(e)})
try:
 model=load('linux_pinned_model_owner',S/'scripts/provision-local-model.py');M=B/'inputs/bundled-model';assert not M.exists()
 for profile in model.CATALOG['profiles']:
  original=W/'artifacts/advanced-split/bundled-model-v3'/profile['id'];target=M/profile['id'];target.mkdir(parents=True)
  rows=[]
  for asset in profile['assets']:
   rows.append(copy(original/asset['name'],target/asset['name'],asset['bytes'],asset['sha256']))
  rawmanifest=original/'model.json';rows.append(copy(rawmanifest,target/'model.json',rawmanifest.stat().st_size,sha(rawmanifest)))
  model.provision(target,download=False,profile=profile)
  record['models'].append({'profile':profile['id'],'assets':rows,'existing_owner_verified':True,'original_manifest_bytes_preserved':True});save()
 record['models_passed']=True
except BaseException as e:record['failures'].append({'stage':'models','type':type(e).__name__,'message':str(e)})
try:
 core=load('linux_pinned_static_owner',S/'scripts/provision-static-onnx-runtime.py');T='x86_64-unknown-linux-gnu';core.validate_lock();D=core.owned_destination(B/'inputs/static-ort',T,create=True)
 original=W/'artifacts/advanced-split/ort130-x64/downloads'
 for name,row in [('source.tar.gz',core.LOCK['source']),*[(r['name']+'.zip',r) for r in core.selected_dependencies(T)],*[(r['cache_name'],r) for r in core.LOCK['additional_notices']]]:
  record['static_archives'].append(copy(original/name,D/'downloads'/name,row['bytes'],row['sha256']));core.verify_file(D/'downloads'/name,row);save()
 tools=load('linux_pinned_static_tools_owner',S/'scripts/provision-static-onnx-tools.py');toolsroot=tools.setup_owner(D,T,core,create=True);ninja=tools.LOCK['ninja'];record['static_archives'].append(copy(W/'artifacts/advanced-split/ort130-x64/tools/downloads/ninja-source.tar.gz',toolsroot/'downloads/ninja-source.tar.gz',ninja['bytes'],ninja['sha256']))
 make=B/'toolchain/bin/make';assert not make.exists();make.symlink_to('../sysroot/usr/bin/make');assert make.resolve().is_relative_to(B)
 record.update(static_archive_stage_passed=True,static_owner_path=str(D/'owner.json'),static_owner_sha256=sha(D/'owner.json'),static_tools_owner_sha256=sha(toolsroot/'owner.json'),static_source_extracted=False,static_or_CMake_or_ninja_build=False,linux_CMake_archive_not_yet_present=True)
except BaseException as e:record['failures'].append({'stage':'static_archives','type':type(e).__name__,'message':str(e)})
record['passed']=all(record[k] for k in ['source_passed','models_passed','static_archive_stage_passed']);record['seconds']=time.monotonic()-began;save();print(json.dumps(record));print('RECEIPT_SHA256',sha(out))
