# SPDX-License-Identifier: Apache-2.0
"""Read-only scope check after authorized isolated prerequisites."""
import json
from pathlib import Path
import subprocess
ROOT=Path(__file__).resolve().parents[3];HERE=Path(__file__).resolve().parent
connection=json.loads((ROOT/'artifacts/v190/neo-connection.local.json').read_bytes())
remote=r'''import hashlib,json,os,pwd,stat
from pathlib import Path
home=Path(pwd.getpwuid(os.geteuid()).pw_dir)
root=home/'airk-builds/advanced-split-local-delivery-prerequisites-r1'
before=json.loads((root/'receipts/before.json').read_bytes())
logs=home/'.npm/_logs'
recent=[]
if logs.is_dir():
 for path in logs.iterdir():
  if path.is_file() and path.stat().st_mtime>=before['started_unix']:
   recent.append({'name':path.name,'bytes':path.stat().st_size,'modified_unix':path.stat().st_mtime})
index=home/'Code/airk/.git/index'
rows={}
for path in [root,root/'tools',root/'inputs',root/'inputs/bundled-model-v2']:
 info=path.stat();rows[str(path)]={'uid':info.st_uid,'mode':oct(stat.S_IMODE(info.st_mode)),'symlink':path.is_symlink()}
print(json.dumps({'schema':'airk.private-neo-prerequisite-footprint/1','read_only':True,'paths':rows,'original_git_index_sha256':hashlib.sha256(index.read_bytes()).hexdigest(),'source_copy_destination_absent':not (home/'airk-builds/advanced-split-local-delivery-r1').exists(),'npm_log_metadata_since_node_start':recent,'no_log_contents_read':True}))
'''
x=subprocess.run(['ssh','-p',str(connection['port']),'-i',str(connection['identity_path']),*connection['ssh_options'],connection['user']+'@'+connection['host'],'/usr/bin/python3','-'],input=remote,capture_output=True,text=True,timeout=30)
receipt={'ssh_exit':x.returncode,'credentials_printed':False}
if x.returncode==0:receipt['observation']=json.loads(x.stdout)
(HERE/'neo-prerequisite-footprint.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
print(json.dumps(receipt));raise SystemExit(x.returncode)
