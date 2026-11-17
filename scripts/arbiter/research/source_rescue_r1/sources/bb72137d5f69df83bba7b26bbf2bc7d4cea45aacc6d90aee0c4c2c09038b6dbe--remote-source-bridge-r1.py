# SPDX-License-Identifier: Apache-2.0

from pathlib import Path
import json,os,sys,stat,hashlib,subprocess,tarfile,importlib.util,time
from datetime import datetime,timezone
started=time.monotonic();uid=os.getuid();assert uid==os.geteuid()==501==os.stat('/dev/console').st_uid and uid!=0
base=Path.home()/'airk-builds/arbiter-final-20261116-r1';source=base/'source';e=base/'evidence';transfer=base/'transfer';old_path=transfer/'bridge-f5f9dcec-r1/source-bridge-manifest.json'
for q in (base,source,e,transfer):assert q.lstat().st_uid==uid and not q.is_symlink() and os.access(q,os.W_OK|os.X_OK)
assert stat.S_IMODE(base.stat().st_mode)&0o077==0
assert not (source/'target').exists()
def sha(q):
 with q.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def git(*args,input=None):return subprocess.check_output(['git','--no-optional-locks',*args],cwd=source,input=input,stderr=subprocess.PIPE)
old=json.loads(old_path.read_text());assert sha(old_path)=='e28111f98e74f71196fa43ed3aacaf32f078d1d6db38c9eaf01195c18364a1e1'
assert git('rev-parse','HEAD').decode().strip()==old['commit']=='f5f9dceceb91ce3d8bace0061522dbf889612c09'
assert not git('status','--porcelain=v1','--untracked-files=all')
before={}
for row in old['files']:
 q=source/row['path'];s=q.lstat();assert stat.S_ISREG(s.st_mode) and s.st_uid==uid and s.st_nlink==1 and sha(q)==row['sha256'];before[row['path']]={'uid':s.st_uid,'mode':stat.S_IMODE(s.st_mode),'inode':s.st_ino,'sha256':row['sha256']}
original=Path.home()/'code/airk';dirty_before=subprocess.check_output(['git','--no-optional-locks','status','--porcelain=v1','-z','--untracked-files=normal'],cwd=original);assert hashlib.sha256(dirty_before).hexdigest()=='943936de2803601aa34eeb0d5c974bbb94261dc762496bd44196177755734b06'
incoming=transfer/'bridge-59580fa6-r1';assert not incoming.exists();incoming.mkdir(mode=0o700)
expected={'source-bridge-manifest.json': {'name': 'source-bridge-manifest.json', 'bytes': 924707, 'sha256': '3809404f750cdadd44ff9dc6e26646e07de07468182847fe28ef11f42512f37a'}, 'raw-source-delta.tar': {'name': 'raw-source-delta.tar', 'bytes': 1198080, 'sha256': '7a0d794ec984ebc111e3cb20b9471b52e1495db897460b69caea4eb9d482c154'}, 'f5f9dcec-to-59580fa6.bundle': {'name': 'f5f9dcec-to-59580fa6.bundle', 'bytes': 20409, 'sha256': 'b70031219a2ebba6de5aa2787ec1b275dad363cf968dbaf2d97366d27c10430f'}}
received=[]
with tarfile.open(fileobj=sys.stdin.buffer,mode='r|gz') as stream:
 for member in stream:
  assert member.name in expected and member.name not in received and member.isfile() and member.size==expected[member.name]['bytes']
  q=incoming/member.name
  with stream.extractfile(member) as src,q.open('xb') as out:
   for block in iter(lambda:src.read(1024*1024),b''):out.write(block)
   out.flush();os.fsync(out.fileno())
  assert sha(q)==expected[member.name]['sha256'];received.append(member.name)
assert set(received)==set(expected)
manifest=json.loads((incoming/'source-bridge-manifest.json').read_text());assert manifest['base_commit']==old['commit'] and manifest['base_raw_c7']==old['raw_c7'] and manifest['commit']=='59580fa6e6742e8101fd35346a2b999621671278'
rows={r['path']:r for r in manifest['files']};change_names=set(manifest['raw_or_mode_changed']);commands=[]
def run(args):
 t=time.monotonic();r=subprocess.run(args,cwd=source,capture_output=True,check=True);commands.append({'argv':args,'cwd':str(source),'exit_code':r.returncode,'elapsed_seconds':time.monotonic()-t,'stdout':r.stdout.decode(),'stderr':r.stderr.decode()});return r.stdout
run(['git','bundle','verify',str(incoming/'f5f9dcec-to-59580fa6.bundle')])
run(['git','fetch','--no-tags',str(incoming/'f5f9dcec-to-59580fa6.bundle'),'HEAD'])
assert git('rev-parse','FETCH_HEAD').decode().strip()==manifest['commit']
run(['git','checkout','--detach',manifest['commit']])
seen=set();overlaid=[]
with tarfile.open(incoming/'raw-source-delta.tar','r') as archive:
 for member in archive:
  assert member.name in rows and member.name in change_names and member.name not in seen and member.isfile() and member.size==rows[member.name]['bytes'];seen.add(member.name)
  q=source/member.name;s=q.lstat();assert stat.S_ISREG(s.st_mode) and s.st_uid==uid and s.st_nlink==1
  payload=archive.extractfile(member).read();assert hashlib.sha256(payload).hexdigest()==rows[member.name]['sha256']
  if q.read_bytes()!=payload:
   with q.open('wb') as out:out.write(payload);out.flush();os.fsync(out.fileno())
   after=q.lstat();assert after.st_uid==s.st_uid and after.st_ino==s.st_ino and after.st_mode==s.st_mode;overlaid.append(member.name)
assert seen==change_names
for name in manifest['deleted']:assert not (source/name).exists()
unchanged=[]
for name,row in rows.items():
 q=source/name;s=q.lstat();assert stat.S_ISREG(s.st_mode) and s.st_uid==uid and s.st_nlink==1 and sha(q)==row['sha256']
 if name in before and name not in change_names:
  assert s.st_ino==before[name]['inode'] and stat.S_IMODE(s.st_mode)==before[name]['mode'];unchanged.append(name)
names=sorted(rows);wire=b''.join(x.encode()+b'\0' for x in names);attrs=git('check-attr','-z','--stdin','filter',input=wire).split(b'\0');assert len(attrs)==len(names)*3+1 and all(attrs[x+2]==b'unspecified' for x in range(0,len(attrs)-1,3))
normal=subprocess.run(['git','--no-optional-locks','hash-object','--stdin-paths'],cwd=source,input=('\n'.join(names)+'\n').encode(),capture_output=True,check=True);blobs=normal.stdout.decode().splitlines();assert len(blobs)==len(names) and all(blob==rows[name]['blob'] for name,blob in zip(names,blobs))
if overlaid:run(['git','add','--',*overlaid])
assert not git('diff','--cached','--name-status','HEAD','--')
assert not git('status','--porcelain=v1','--untracked-files=all')
assert git('rev-parse','HEAD^{tree}').decode().strip()==manifest['normalized_tree']
assert set(os.fsdecode(x) for x in git('ls-files','-z').split(b'\0') if x)==set(rows)
spec=importlib.util.spec_from_file_location('actual_bridge_native_c7',source/'scripts/build-service-runtime.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);seal=module.source_identity();assert seal==manifest['raw_c7']
dirty_after=subprocess.check_output(['git','--no-optional-locks','status','--porcelain=v1','-z','--untracked-files=normal'],cwd=original);assert dirty_after==dirty_before
absent=[str(q.relative_to(source)) for q in [source/'target',source/'packages/vscode/out',source/'packages/vscode/node_modules',source/'packages/admin/node_modules'] if not q.exists()];assert len(absent)==4
proof={'schema':'airk.neo-authentic-source-bridge-proof/1','utc':datetime.now(timezone.utc).isoformat(),'elapsed_seconds':time.monotonic()-started,'uid':uid,'console_uid':os.stat('/dev/console').st_uid,'base_commit':old['commit'],'commit':manifest['commit'],'normalized_tree':manifest['normalized_tree'],'raw_c7':seal,'source_files_verified':len(rows),'source_delta_files_received':len(change_names),'raw_overlay_files':len(overlaid),'deleted_public_paths':manifest['deleted'],'unchanged_source_file_ids_modes_preserved':len(unchanged),'all_current_source_files_regular_owner_uid':uid,'source_clean':True,'staged_tree_unchanged_by_raw_overlay':True,'full_staged_catalog_sha256':hashlib.sha256(git('ls-files','--stage','-z')).hexdigest(),'original_dirty_checkout_preserved':True,'original_dirty_status_sha256':hashlib.sha256(dirty_after).hexdigest(),'original_dirty_entries':365,'old_source_custody_retained':True,'old_c7_not_retagged':old['raw_c7'],'transfer_files':expected,'public_source_inventory':{'manifest_sha256':expected['source-bridge-manifest.json']['sha256'],'source_files':len(rows)},'commands':commands,'ignored_outputs_absent':absent,'Windows_observer_transferred':False,'Cargo_or_build_invoked':False,'static_and_model_inputs_rebuilt_or_changed':False,'scope':'Only genuine Git transition and reviewed current public source raw overlay; no target, package, install/service, model or ORT build.'}
with (e/'source-bridge-59580fa6-r1.json').open('x') as f:json.dump(proof,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
print(json.dumps(proof,indent=2),flush=True)
