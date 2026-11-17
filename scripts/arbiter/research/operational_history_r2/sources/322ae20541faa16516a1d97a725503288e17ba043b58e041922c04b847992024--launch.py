# SPDX-License-Identifier: Apache-2.0
import hashlib,json,shlex,subprocess,tarfile,time
from pathlib import Path
root=Path.cwd();base=root/'artifacts/local/1.8.12-explorer-20261107/mac-neo'
c=json.loads((root/'artifacts/v190/neo-connection.local.json').read_text(encoding='utf-8-sig'))
ssh=['ssh','-i',c['identity_path'],*c['ssh_options']];scp=['scp','-i',c['identity_path'],*c['ssh_options']]
if c.get('port'):ssh+=['-p',str(c['port'])];scp+=['-P',str(c['port'])]
address=c['user']+'@'+c['host'];remote='/Users/tylerjensen/airk-builds/explorer-1812-20261107-'+str(int(time.time()))
record=json.loads((base/'snapshot.json').read_text());delta={}
names=['scripts/package-extension.py','scripts/promote-vsix.py','scripts/tests/test_package_extension.py','scripts/tests/test_release_promotion.py']
with tarfile.open(base/'overlay.tar.gz','x:gz') as out:
 for n in names:
  now=hashlib.sha256((root/n).read_bytes()).hexdigest();before=record['files'][n]
  if now!=before:delta[n]={'before':before,'after':now};out.add(root/n,arcname=n,recursive=False)
(base/'overlay.json').write_text(json.dumps({'files':delta,'archive_sha256':hashlib.sha256((base/'overlay.tar.gz').read_bytes()).hexdigest()},indent=2)+'\n',encoding='utf-8',newline='\n')
subprocess.run([*ssh,address,'umask 077; mkdir '+shlex.quote(remote)],check=True)
for n in ['source.tar.gz','snapshot.json','git-metadata.json','owner.py','overlay.tar.gz','overlay.json']:
 subprocess.run([*scp,str(base/n),address+':'+remote+'/'+n],check=True)
python=c['existing_tools']+'/python3.13'
command='cd '+shlex.quote(remote)+' && nohup '+shlex.quote(python)+' owner.py > owner.log 2>&1 < /dev/null &'
subprocess.run([*ssh,address,command],check=True)
(base/'active.json').write_text(json.dumps({'remote':remote,'native_source':record['native_source'],'archive_sha256':record['archive_sha256'],'public_files':len(record['files']),'overlay':delta,'cleanup':'owner.py finally preserves evidence and cargo-cleans exact isolated target'},indent=2)+'\n',encoding='utf-8',newline='\n')
print(json.dumps({'launched':True,'remote':remote,'native_source':record['native_source'],'public_files':len(record['files']),'overlay_files':list(delta)}))
