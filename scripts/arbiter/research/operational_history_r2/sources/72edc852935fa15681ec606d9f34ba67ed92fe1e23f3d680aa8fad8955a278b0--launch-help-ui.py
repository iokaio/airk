# SPDX-License-Identifier: Apache-2.0
import hashlib,json,shlex,subprocess,tarfile
from pathlib import Path
root=Path.cwd();base=root/'artifacts/local/1.8.12-explorer-20261107/mac-neo';c=json.loads((root/'artifacts/v190/neo-connection.local.json').read_text(encoding='utf-8-sig'));active=json.loads((base/'active.json').read_text());snapshot=json.loads((base/'snapshot.json').read_text())
names=['packages/vscode/src/explorerHelp/recovering-errors.md','scripts/check-extension-update-editor.cjs'];delta={}
with tarfile.open(base/'help-ui-delta.tar.gz','x:gz') as out:
 for n in names:delta[n]={'before':snapshot['files'][n],'after':hashlib.sha256((root/n).read_bytes()).hexdigest()};out.add(root/n,arcname=n,recursive=False)
(base/'help-ui-delta.json').write_text(json.dumps({'files':delta,'archive_sha256':hashlib.sha256((base/'help-ui-delta.tar.gz').read_bytes()).hexdigest()},indent=2)+'\n',encoding='utf-8',newline='\n')
address=c['user']+'@'+c['host'];remote=active['remote']
for n in ['help-ui-owner.py','help-ui-delta.json','help-ui-delta.tar.gz']:
 subprocess.run(['scp','-i',c['identity_path'],*c['ssh_options'],str(base/n),address+':'+remote+'/'+n],check=True)
cmd='cd '+shlex.quote(remote)+' && nohup '+shlex.quote(c['existing_tools']+'/python3.13')+' help-ui-owner.py > help-ui-owner.log 2>&1 < /dev/null &'
subprocess.run(['ssh','-i',c['identity_path'],*c['ssh_options'],address,cmd],check=True)
print(json.dumps({'launched':True,'native_rebuild':False,'delta':delta}))
