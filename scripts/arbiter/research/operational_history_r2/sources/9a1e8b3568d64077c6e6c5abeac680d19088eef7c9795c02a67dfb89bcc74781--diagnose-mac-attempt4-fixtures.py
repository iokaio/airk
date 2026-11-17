# SPDX-License-Identifier: Apache-2.0
import base64,hashlib,json,subprocess
from pathlib import Path
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
remote='''set -eu
cd "$HOME/code/airk"
/usr/bin/python3 - <<'PY'
import base64,hashlib,json,os,tempfile
from pathlib import Path
result={'temp_directory':tempfile.gettempdir(),'temp_directory_canonical':str(Path(tempfile.gettempdir()).resolve()),'var_is_symlink':Path('/var').is_symlink(),'files':{}}
for name,path in [('observed',Path(tempfile.gettempdir())/'airk-arbiter.schema.observed.json'),('existing_golden',Path('packages/contracts-gen/arbiter.schema.json'))]:
 if path.is_file():
  data=path.read_bytes();assert len(data)<1024*1024;result['files'][name]={'sha256':hashlib.sha256(data).hexdigest(),'content':base64.b64encode(data).decode()}
flags=os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW
for name in ['/var','/private/var']:
 try:fd=os.open(name,flags);os.close(fd);result[name]='opened-no-follow'
 except OSError as error:result[name]={'errno':error.errno,'message':str(error)}
print(json.dumps(result))
PY
'''
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=35)
if r.returncode:raise SystemExit(r.stderr.decode())
v=json.loads(r.stdout);out=Path('artifacts/advanced-split/receipts/mac-platform-scoped-attempt4')
for name,row in v['files'].items():(out/('arbiter-schema-'+name+'.json')).write_bytes(base64.b64decode(row['content']))
local=json.loads(Path('packages/contracts-gen/arbiter.schema.json').read_text());v['observed_matches_current_reviewed_golden']=json.loads(base64.b64decode(v['files']['observed']['content']))==local
summary={k:value for k,value in v.items() if k!='files'};summary['observed_sha256']=v['files']['observed']['sha256'];summary['remote_golden_present']='existing_golden' in v['files'];(out/'tmpdir-and-schema-diagnosis.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8');print(json.dumps(summary,indent=2))
