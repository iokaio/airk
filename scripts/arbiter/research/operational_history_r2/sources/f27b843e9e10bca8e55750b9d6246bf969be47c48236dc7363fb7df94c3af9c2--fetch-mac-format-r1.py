# SPDX-License-Identifier: Apache-2.0
import base64,difflib,hashlib,json,subprocess
from pathlib import Path
plan_path=Path('artifacts/advanced-split/mac-platform-source-plan.json');plan=json.loads(plan_path.read_text());c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
remote='''set -eu
cd "$HOME/code/airk"
/usr/bin/python3 - <<'PY'
import base64,hashlib,json,tarfile
from pathlib import Path
archive=Path('artifacts/advanced-split/receipts/mac-platform-sync-7d913a63b20692b4/source.tar.gz')
with tarfile.open(archive,'r:gz') as source:manifest=json.loads(source.extractfile('.sync-manifest.json').read())
changed={}
for name,row in manifest['files'].items():
 p=Path(name);content=p.read_bytes();sha=hashlib.sha256(content).hexdigest()
 if sha!=row['sha256']:changed[name]={'before_sha256':row['sha256'],'after_sha256':sha,'bytes':base64.b64encode(content).decode()}
print(json.dumps({'source_seal_before':manifest['seal'],'changed':changed}))
PY
'''
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=45)
if r.returncode:raise SystemExit(r.stderr.decode())
result=json.loads(r.stdout);retained=Path('artifacts/advanced-split/receipts/mac-format-r1');retained.mkdir(exist_ok=False);(retained/'formatted-source.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
patch=[];applied=[];conflicts=[]
for name,row in result['changed'].items():
 if not name.startswith('crates/') or not name.endswith('.rs'):raise ValueError('Unexpected non-Rust formatting change: '+name)
 path=Path(name);current=path.read_bytes();new=base64.b64decode(row['bytes'])
 if hashlib.sha256(new).hexdigest()!=row['after_sha256']:raise ValueError('Fetched source differs')
 # Original preimage is retained in the deterministic source archive, never reconstructed.
 import tarfile
 with tarfile.open('artifacts/advanced-split/receipts/mac-platform-source-7d913a63b20692b4.tar.gz') as archive:before=archive.extractfile(name).read()
 assert hashlib.sha256(before).hexdigest()==row['before_sha256']
 patch.extend(difflib.unified_diff(before.decode('utf-8').splitlines(keepends=True),new.decode('utf-8').splitlines(keepends=True),fromfile='a/'+name,tofile='b/'+name))
 if hashlib.sha256(current).hexdigest()==row['before_sha256']:
  backup=retained/'windows-preimage'/name;backup.parent.mkdir(parents=True,exist_ok=True);backup.write_bytes(current);path.write_bytes(new);applied.append(name)
 elif hashlib.sha256(current).hexdigest()==row['after_sha256']:applied.append(name)
 else:conflicts.append({'path':name,'current_sha256':hashlib.sha256(current).hexdigest(),'before_sha256':row['before_sha256'],'formatted_sha256':row['after_sha256']})
(retained/'format-only.patch').write_text(''.join(patch),encoding='utf-8',newline='\n');report={'changed':len(result['changed']),'applied':applied,'conflicts_preserved':conflicts,'root_action':'cargo fmt --all only, exit0, mac-platform-source-format-r1.log','classification':'mechanical Rust formatting from original frozen Mac preimages; reviewable exact before/after patch retained','source_policy_paths':[name for name in result['changed'] if 'source_policy' in name or 'source-policy' in name]};(retained/'application.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8');print(json.dumps(report,indent=2))
