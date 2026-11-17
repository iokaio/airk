# SPDX-License-Identifier: Apache-2.0
import argparse,hashlib,json,shlex,subprocess,tarfile
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--label',required=True);p.add_argument('--packages',action='store_true');p.add_argument('--package-kind',action='append');a=p.parse_args()
assert a.label and all(c.isalnum() or c=='-' for c in a.label)
base=Path('artifacts/local/1.8.12-explorer-20261107/mac-neo');c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text(encoding='utf-8-sig'));active=json.loads((base/'active.json').read_text())
ssh=['ssh','-i',c['identity_path'],*c['ssh_options']]
if c.get('port'):ssh+=['-p',str(c['port'])]
destination=base/a.label;destination.mkdir(exist_ok=False)
script='from pathlib import Path\nimport tarfile,sys\nr=Path('+repr(active['remote'])+')\npackages='+repr(a.packages)+'\npackage_kinds='+repr(a.package_kind or ['native','vscode'])+'\n'+'''
with tarfile.open(fileobj=sys.stdout.buffer,mode='w|gz') as out:
 for name in ['owner.log','snapshot.json','steps.json','prerequisites.json','qualification.json','cleanup.json','applied-overlay.json','snapshot-after-reviewed-overlay.json','ui-followup.json','phase.txt','help-ui-owner.log','help-ui-delta.json','help-ui-steps.json','help-ui-followup.json','help-ui-phase.txt','sign-retry-owner.log','sign-retry-result.json','sign-retry-steps.json','sign-retry-phase.txt','ui2-owner.py.log','ui2-delta.json','ui2-steps.json','ui2-result.json','ui2-phase.txt','gui-keychain-probe.py.log','gui-sign-owner.log','gui-sign-result.json','gui-sign-phase.txt']:
  p=r/name
  if p.is_file():out.add(p,arcname=name,recursive=False)
 for p in sorted((r/'evidence').glob('*')):
  if p.is_file() and not p.is_symlink() and p.suffix in ['.log','.json','.html']:out.add(p,arcname='evidence/'+p.name,recursive=False)
 for kind in ['installed-explorer-preflight','editor-update-preflight']:
  for result in sorted((r/'source/artifacts'/kind).glob('run-*/result.json')):
   if result.is_file() and not result.is_symlink():out.add(result,arcname='ui-results/'+kind+'/'+result.parent.name+'/result.json',recursive=False)
  for log in sorted((r/'source/artifacts'/kind).glob('run-*/*.log')):
   if log.is_file() and not log.is_symlink():out.add(log,arcname='ui-results/'+kind+'/'+log.parent.name+'/'+log.name,recursive=False)
 if packages:
  for kind in package_kinds:
   for p in sorted((r/'source/artifacts/neo'/kind/'publish').glob('*')):
    if p.is_file() and p.suffix in ['.json','.zip','.vsix','.txt'] or (p.is_file() and p.name=='SHA256SUMS'):out.add(p,arcname='packages/'+kind+'/'+p.name,recursive=False)
  for name in ['airk-build.json','airk-install.json','airk-runtime.json']:
   p=r/'source/artifacts/neo/runtime'/name
   if p.is_file():out.add(p,arcname='runtime-metadata/'+name,recursive=False)
  for sign_kind in ['signed','signed-gui1']:
   signed=r/'source/artifacts/neo'/sign_kind
   for p in sorted(signed.rglob('*.json')) if signed.is_dir() else []:
    if p.is_file() and not p.is_symlink():out.add(p,arcname='signing/'+sign_kind+'/'+p.relative_to(signed).as_posix(),recursive=False)
'''
command=shlex.quote(c['existing_tools']+'/python3.13')+' -c '+shlex.quote(script)
archive=destination/'evidence.tar.gz'
with archive.open('xb') as out:
 result=subprocess.run([*ssh,c['user']+'@'+c['host'],command],stdout=out,stderr=subprocess.PIPE,timeout=180)
if result.returncode:raise RuntimeError(result.stderr.decode(errors='replace'))
assert archive.stat().st_size<=350*1024*1024
with tarfile.open(archive) as source:
 members=source.getmembers();assert all(m.isfile() and not Path(m.name).is_absolute() and (destination/m.name).resolve().is_relative_to(destination.resolve()) for m in members)
 source.extractall(destination,filter='data')
files={p.relative_to(destination).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in destination.rglob('*') if p.is_file()}
(destination/'collection-manifest.json').write_text(json.dumps({'schema':'airk.neo-evidence-collection/1','remote':active['remote'],'files':files},indent=2)+'\n',encoding='utf-8',newline='\n')
print(json.dumps({'destination':str(destination),'files':len(files),'archive_bytes':archive.stat().st_size,'packages':a.packages}))
