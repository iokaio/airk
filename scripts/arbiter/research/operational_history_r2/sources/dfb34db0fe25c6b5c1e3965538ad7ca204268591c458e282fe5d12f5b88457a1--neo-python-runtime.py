# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
remote='''set -eu
cd "$HOME/code/airk"
/usr/bin/python3 - <<'PY'
from pathlib import Path
import json,subprocess,os
import hashlib
root=Path('artifacts/tools/neo-python-3.13.18').resolve();receipt=json.loads((root/'provision-receipt.json').read_text());binaries=[]
for name,row in receipt['files'].items():
 if row['kind']!='file':continue
 p=root/name
 with p.open('rb') as source:magic=source.read(4)
 if magic.hex() in ('cffaedfe','feedfacf','cafebabe','bebafeca'):
  assert hashlib.sha256(p.read_bytes()).hexdigest()==row['sha256'];binaries.append({'path':name,'bytes':row['bytes'],'sha256':row['sha256']})
r=subprocess.run(['/usr/bin/otool','-L',*[str(root/row['path']) for row in binaries]],capture_output=True,timeout=30)
assert r.returncode==0
ca=Path('/private/etc/ssl/cert.pem');env=Path('artifacts/advanced-split/ort130-arm64-env.json').resolve()
record={'schema':'airk.private-build-tool-runtime/1','root':str(root),'provision_receipt_sha256':hashlib.sha256((root/'provision-receipt.json').read_bytes()).hexdigest(),'binary_inventory':binaries,'dynamic_dependencies':r.stdout.decode(),'certificate_roots_sha256':hashlib.sha256(ca.read_bytes()).hexdigest(),'existing_certificate_roots':str(ca),'ort_environment_file':str(env),'ort_environment':json.loads(env.read_text())}
output=Path('artifacts/advanced-split/receipts/neo-python-3.13.18-runtime.json')
with output.open('x') as stream:json.dump(record,stream,indent=2)
print(json.dumps(record))
PY
'''
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=45)
Path('artifacts/advanced-split/receipts/delivery-04-neo-python-runtime.log').write_bytes(r.stdout+r.stderr)
print(r.stdout.decode());print(r.stderr.decode());raise SystemExit(r.returncode)
