# SPDX-License-Identifier: Apache-2.0
import os,pathlib,json,hashlib,subprocess,tarfile,io,time
P=pathlib.Path(os.environ['AIRK_LINUX_TRANSFER_PROPOSAL']);proposal=json.loads(P.read_text())
W=pathlib.Path(proposal['windows_root']);D=pathlib.Path(proposal['windows_snapshot']);deadline=time.monotonic()+600
def check():
 if time.monotonic()>=deadline:raise TimeoutError('original source-pack deadline')
def sha(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  while data:=f.read(1<<20):check();h.update(data)
 return h.hexdigest()
def run(argv):
 check();p=subprocess.run(argv,cwd=W,capture_output=True,timeout=min(90,deadline-time.monotonic()),check=True);check();return p.stdout
def verify():
 assert run(['git','rev-parse','HEAD']).decode().strip()==proposal['base_commit']
 assert run(['git','status','--porcelain=v1','-z','--untracked-files=all']).hex()==proposal['status_hex']
 for row in proposal['files']:
  p=W/row['path'];assert p.is_file() and not p.is_symlink() and p.resolve().is_relative_to(W) and p.stat().st_size==row['bytes'] and sha(p)==row['sha256'],row['path']
def custody(path,created=False):
 assert sha(pathlib.Path(proposal['guard']['path']))==proposal['guard']['sha256']
 assert sha(pathlib.Path(proposal['powershell']['path']))==proposal['powershell']['sha256']
 environment=dict(os.environ,AIRK_COPY_GUARD=proposal['guard']['path'],AIRK_COPY_PATH=str(path),AIRK_COPY_ROOT=str(W))
 function='Assert-AirkCreatedOutput' if created else 'Assert-AirkOutputParent'
 script="$ErrorActionPreference='Stop'; . $env:AIRK_COPY_GUARD; "+function+" -Path $env:AIRK_COPY_PATH -Root $env:AIRK_COPY_ROOT -ExpectedSid S-1-5-21-3678370870-3858642417-826968105-1002 | Out-Null"
 subprocess.run([proposal['powershell']['path'],'-NoProfile','-NonInteractive','-Command',script],env=environment,check=True,capture_output=True,timeout=min(90,deadline-time.monotonic()));check()
verify();assert D.is_dir() and not list(D.iterdir());custody(D,True)
for name in ('source.bundle','raw-source.tar','tracked-overlay.patch','source-manifest.json','custody.json'):custody(D/name)
run(['git','bundle','create',str(D/'source.bundle'),'refs/heads/feat/arbiter-final'])
custody(D/'source.bundle',True)
assert (D/'source.bundle').stat().st_size<=512<<20
with (D/'raw-source.tar').open('xb') as stream,tarfile.open(fileobj=stream,mode='w') as archive:
 for row in proposal['files']:
  check();p=W/row['path'];entry=tarfile.TarInfo(row['path']);entry.size=row['bytes'];entry.mode=0o700 if row['mode']=='100755' else 0o600;entry.uid=entry.gid=1000;entry.mtime=0
  with p.open('rb') as source:archive.addfile(entry,source)
custody(D/'raw-source.tar',True)
assert (D/'raw-source.tar').stat().st_size<=512<<20
patch=run(['git','diff','--binary','HEAD','--'])
assert hashlib.sha256(patch).hexdigest()==proposal['tracked_patch_sha256']
with (D/'tracked-overlay.patch').open('xb') as f:f.write(patch)
custody(D/'tracked-overlay.patch',True)
verify()
manifest={k:proposal[k] for k in ('schema','base_commit','base_tree','native_group','native_receipt','files','overlay','status_hex','tracked_patch_sha256','raw_file_census_sha256')}
manifest.update(git_clean=False,source_identity='base commit plus explicit reviewed dirty overlay',proposal_sha256=sha(P))
with (D/'source-manifest.json').open('x',encoding='utf-8') as f:json.dump(manifest,f,indent=2);f.write('\n')
custody(D/'source-manifest.json',True)
artifacts={p.name:{'bytes':p.stat().st_size,'sha256':sha(p)} for p in D.iterdir()}
assert sum(r['bytes'] for r in artifacts.values())<=1<<30
receipt={'schema':'airk.private.linux-source-pack/1','artifacts':artifacts,'source_unchanged':True,'git_clean':False,'base_commit':proposal['base_commit'],'native_group':proposal['native_group'],'elapsed_seconds':600-(deadline-time.monotonic())}
check()
with (D/'custody.json').open('x',encoding='utf-8') as f:json.dump(receipt,f,indent=2);f.write('\n')
custody(D/'custody.json',True)
print(json.dumps(receipt))
