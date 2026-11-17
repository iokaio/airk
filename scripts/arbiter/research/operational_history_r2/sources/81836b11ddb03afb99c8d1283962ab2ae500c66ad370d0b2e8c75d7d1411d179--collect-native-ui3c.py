# SPDX-License-Identifier: Apache-2.0
import hashlib,json,shlex,subprocess,tarfile
from pathlib import Path
b=Path(__file__).resolve().parent;c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text(encoding='utf-8-sig'));r=json.loads((b/'notary-active.json').read_text())['remote'];destination=b/'native-ui-3c-same1-complete';destination.mkdir(exist_ok=False)
code='from pathlib import Path\nimport json,tarfile,sys,os\nr=Path('+repr(r)+')\n'+'''
receipt=json.loads((r/'evidence3c/native-ui3c-result.json').read_text());owned=Path(receipt['editor_run']);assert owned.resolve().is_relative_to(r/'source3c6e81c/artifacts/editor-qualification')
with tarfile.open(fileobj=sys.stdout.buffer,mode='w|gz') as out:
 for n in ['native-ui-owner3c.log','native-ui3c-phase.txt','evidence3c/native-ui3c-result.json','evidence3c/native-enabled-installed-same3c.log']:
  p=r/n
  if p.is_file():out.add(p,arcname=n,recursive=False)
 # Only test reports/logs/screenshots; never copy runtime/index/database state.
 for folder in [owned,owned/'same']:
  for p in sorted(folder.iterdir()):
   if p.is_file() and not p.is_symlink() and p.suffix in ['.json','.log','.png']:out.add(p,arcname='editor-run/'+p.relative_to(owned).as_posix(),recursive=False)
 # Retained profiles: bounded diagnostic logs only, not settings/storage/state.
 for marker in sorted((owned/'same').glob('profile-*-owned.json')):
  p=Path(json.loads(marker.read_text())['path']);assert p.parent==Path('/private/tmp') and p.name.startswith('airk-native-')
  if not p.exists():continue
  total=0
  for log in sorted((p/'logs').rglob('*')):
   if log.is_file() and not log.is_symlink() and log.suffix in ['.json','.log'] and log.stat().st_size<2*1024**2 and total+log.stat().st_size<10*1024**2:
    total+=log.stat().st_size;out.add(log,arcname='retained-profile-logs/'+p.name+'/'+log.relative_to(p/'logs').as_posix(),recursive=False)
'''
archive=destination/'evidence.tar.gz'
with archive.open('xb') as stream:p=subprocess.run(['ssh','-i',c['identity_path'],*c['ssh_options'],c['user']+'@'+c['host'],shlex.quote(c['existing_tools']+'/python3.13')+' -c '+shlex.quote(code)],stdout=stream,stderr=subprocess.PIPE,timeout=120)
assert p.returncode==0,p.stderr.decode(errors='replace');assert archive.stat().st_size<50*1024**2
with tarfile.open(archive) as z:
 assert all(m.isfile() and not Path(m.name).is_absolute() and (destination/m.name).resolve().is_relative_to(destination.resolve()) for m in z.getmembers());z.extractall(destination,filter='data')
files={p.relative_to(destination).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in destination.rglob('*') if p.is_file()};(destination/'collection-manifest.json').write_text(json.dumps({'schema':'airk.neo-native-editor-evidence/1','remote':r,'files':files},indent=2)+'\n');print(json.dumps({'destination':str(destination),'files':len(files),'archive_bytes':archive.stat().st_size}))
