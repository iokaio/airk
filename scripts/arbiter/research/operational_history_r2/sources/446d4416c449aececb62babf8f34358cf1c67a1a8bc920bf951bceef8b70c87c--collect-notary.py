# SPDX-License-Identifier: Apache-2.0
import hashlib,json,shlex,subprocess,tarfile
from pathlib import Path
b=Path(__file__).resolve().parent;c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text(encoding='utf-8-sig'));r=json.loads((b/'notary-active.json').read_text())['remote'];destination=b/'notarized-current-complete';destination.mkdir(exist_ok=False)
code='from pathlib import Path\nimport tarfile,sys\nr=Path('+repr(r)+')\n'+'''
with tarfile.open(fileobj=sys.stdout.buffer,mode='w|gz') as archive:
 for pattern in ['notary*-result.json','notary*-steps.json','notary*-phase.txt','owner*.log','phase.txt']:
  for p in sorted(r.glob(pattern)):
   if p.is_file() and not p.is_symlink():archive.add(p,arcname=p.name,recursive=False)
 for p in sorted((r/'evidence').glob('*')):
  if p.is_file() and not p.is_symlink() and p.suffix in ['.json','.log']:archive.add(p,arcname='evidence/'+p.name,recursive=False)
 for kind in ['native-notarized','vscode-notarized']:
  for p in sorted((r/'source/artifacts/neo'/kind/'publish').glob('*')):
   if p.is_file() and not p.is_symlink():archive.add(p,arcname='packages/'+kind+'/'+p.name,recursive=False)
 signed=r/'source/artifacts/neo/signed-notarized-r2'
 for n in ['signing.json','certificate-identity.json','runtime/airk-build.json','runtime/airk-install.json','runtime/airk-runtime.json']:
  p=signed/n
  if p.is_file():archive.add(p,arcname='signing/'+n,recursive=False)
'''
archive=destination/'evidence.tar.gz'
with archive.open('xb') as out:
 p=subprocess.run(['ssh','-i',c['identity_path'],*c['ssh_options'],c['user']+'@'+c['host'],shlex.quote(c['existing_tools']+'/python3.13')+' -c '+shlex.quote(code)],stdout=out,stderr=subprocess.PIPE,timeout=180)
assert p.returncode==0,p.stderr.decode(errors='replace');assert archive.stat().st_size<350*1024**2
with tarfile.open(archive) as z:
 assert all(m.isfile() and not Path(m.name).is_absolute() and (destination/m.name).resolve().is_relative_to(destination.resolve()) for m in z.getmembers());z.extractall(destination,filter='data')
files={str(p.relative_to(destination)).replace('\\','/'):hashlib.sha256(p.read_bytes()).hexdigest() for p in destination.rglob('*') if p.is_file()}
(destination/'collection-manifest.json').write_text(json.dumps({'schema':'airk.neo-notarized-evidence-collection/1','remote':r,'files':files},indent=2)+'\n')
print(json.dumps({'destination':str(destination),'files':len(files),'archive_bytes':archive.stat().st_size}))
