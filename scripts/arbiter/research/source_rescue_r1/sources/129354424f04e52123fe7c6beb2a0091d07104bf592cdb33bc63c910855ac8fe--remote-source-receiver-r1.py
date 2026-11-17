# SPDX-License-Identifier: Apache-2.0

import os,sys,stat,json,hashlib,tarfile,subprocess,importlib.util,time
from pathlib import Path
from datetime import datetime,timezone
started=time.monotonic();uid=os.getuid();assert uid==501==os.stat('/dev/console').st_uid and uid!=0
home=Path.home();base=home/'airk-builds/arbiter-final-20261116-r1';assert not base.exists()
for parent in (home,home/'airk-builds'):
 assert parent.lstat().st_uid==uid and not parent.is_symlink() and os.access(parent,os.W_OK|os.X_OK)
base.mkdir(mode=0o700);transfer=base/'transfer';transfer.mkdir();evidence=base/'evidence';evidence.mkdir();(base/'inputs').mkdir()
expected={'source-manifest.json': {'name': 'source-manifest.json', 'bytes': 939667, 'sha256': '290ab745b53ca03265d2b0db1f6ef96f446ff45bef25b92ce695782698488c13'}, 'public-source-raw.tar': {'name': 'public-source-raw.tar', 'bytes': 143953920, 'sha256': 'fbe3eebae467c2ea8ab6c5ba9631e815f535f43e3feb0e4940ae3f30b777c4e8'}, 'milestone-1933.bundle': {'name': 'milestone-1933.bundle', 'bytes': 80071144, 'sha256': '37df895b962c9589230a7e067e5075fbd6c653deebe687a7963bf886e0cb9ace'}}
received=[]
with tarfile.open(fileobj=sys.stdin.buffer,mode='r|gz') as incoming:
 for member in incoming:
  assert member.name in expected and member.isfile() and member.size==expected[member.name]['bytes'] and member.name not in received
  path=transfer/member.name;sha=hashlib.sha256()
  with incoming.extractfile(member) as src,path.open('xb') as out:
   for block in iter(lambda:src.read(1024*1024),b''):sha.update(block);out.write(block)
   out.flush();os.fsync(out.fileno())
  assert sha.hexdigest()==expected[member.name]['sha256'];received.append(member.name)
assert set(received)==set(expected)
manifest=json.loads((transfer/'source-manifest.json').read_text());source=base/'source';commands=[]
def run(args,cwd=None):
 start=time.monotonic();r=subprocess.run(args,cwd=cwd,capture_output=True,check=True)
 commands.append({'argv':args,'cwd':str(cwd) if cwd else None,'exit_code':r.returncode,'elapsed_seconds':time.monotonic()-start,'stdout':r.stdout.decode(),'stderr':r.stderr.decode()});return r.stdout
run(['git','bundle','verify',str(transfer/'milestone-1933.bundle')],home/'code/airk')
run(['git','clone','--no-checkout',str(transfer/'milestone-1933.bundle'),str(source)])
run(['git','checkout','--detach',manifest['commit']],source)
rows={r['path']:r for r in manifest['files']};seen=set();changed=[]
with tarfile.open(transfer/'public-source-raw.tar','r') as raw:
 for member in raw:
  assert member.name in rows and member.name not in seen and member.isfile() and member.size==rows[member.name]['bytes'];seen.add(member.name)
  dest=source/member.name;meta=dest.lstat();assert stat.S_ISREG(meta.st_mode) and meta.st_uid==uid and meta.st_nlink==1
  for parent in dest.parents:
   if parent==source.parent:break
   assert not parent.is_symlink() and parent.lstat().st_uid==uid
  payload=raw.extractfile(member).read();assert hashlib.sha256(payload).hexdigest()==rows[member.name]['sha256']
  if dest.read_bytes()!=payload:
   with dest.open('wb') as out:out.write(payload);out.flush();os.fsync(out.fileno())
   assert dest.stat().st_ino==meta.st_ino and dest.stat().st_uid==meta.st_uid and stat.S_IMODE(dest.stat().st_mode)==stat.S_IMODE(meta.st_mode);changed.append(member.name)
assert seen==set(rows)
names=sorted(rows)
filtered=subprocess.run(['git','--no-optional-locks','hash-object','--stdin-paths'],cwd=source,input=('\n'.join(names)+'\n').encode(),capture_output=True,check=True)
assert all(blob==rows[name]['blob'] for name,blob in zip(names,filtered.stdout.decode().splitlines())) and len(filtered.stdout.splitlines())==len(names)
# Only real Git's stat-cache update, after every actual clean-filtered blob
# has independently proved the reviewed committed identity; no synthetic flags.
if changed:run(['git','add','--',*changed],source)
assert not run(['git','--no-optional-locks','status','--porcelain=v1','--untracked-files=normal'],source)
assert run(['git','rev-parse','HEAD^{tree}'],source).decode().strip()==manifest['normalized_tree']
assert not run(['git','diff','--cached','--name-status','HEAD','--'],source)
assert len(run(['git','ls-files','-z'],source).split(bytes([0])))-1==len(rows)
for name,row in rows.items():
 dest=source/name;assert hashlib.sha256(dest.read_bytes()).hexdigest()==row['sha256'] and dest.stat().st_uid==uid
spec=importlib.util.spec_from_file_location('actual_native_source',source/'scripts/build-service-runtime.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);seal=module.source_identity();assert seal==manifest['raw_c7']
dirty=subprocess.check_output(['git','--no-optional-locks','status','--porcelain=v1','-z','--untracked-files=normal'],cwd=home/'code/airk');assert hashlib.sha256(dirty).hexdigest()=='943936de2803601aa34eeb0d5c974bbb94261dc762496bd44196177755734b06'
receipt={'schema':'airk.neo-source-custody/1','utc_completed':datetime.now(timezone.utc).isoformat(),'uid':uid,'console_uid':os.stat('/dev/console').st_uid,'source':str(source),'commit':manifest['commit'],'normalized_tree':manifest['normalized_tree'],'source_files':len(rows),'raw_overlay_files':len(changed),'all_source_raw_hashes_equal':True,'all_output_owner_uid':uid,'raw_c7':seal,'genuine_git_bundle_sha256':expected['milestone-1933.bundle']['sha256'],'received':expected,'source_clean':True,'staged_tree_unchanged':True,'dirty_original_unchanged':True,'dirty_original_entries':365,'dirty_original_status_sha256':hashlib.sha256(dirty).hexdigest(),'commands':commands,'elapsed_seconds':time.monotonic()-started}
with (evidence/'source-custody-r1.json').open('x') as f:json.dump(receipt,f,indent=2);f.write('\n')
print(json.dumps({k:v for k,v in receipt.items() if k!='commands'},indent=2),flush=True)
