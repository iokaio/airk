# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
remote='''set -eu
cd "$HOME/code/airk"
PYTHONDONTWRITEBYTECODE=1 artifacts/tools/neo-python-3.13.18/bin/python3.13 -I -B - <<'PY'
import hashlib,importlib.util,json,os,re,tarfile,time,urllib.request
from pathlib import Path,PurePosixPath
root=Path('artifacts/tools/neo-nextest-0.9.148').resolve();files={}
for name,blob in [('LICENSE-APACHE','16fe87b06e802f094b3fbb0894b137bca2b16ef1'),('LICENSE-MIT','357091078348bb6d2be30c5330180e8181ff759a')]:
 url='https://raw.githubusercontent.com/nextest-rs/nextest/cd1d6d5467322dbc0d39a80cb671d000ac302798/'+name
 with urllib.request.urlopen(url,timeout=15) as response:content=response.read(16385)
 assert len(content)<=16384
 assert hashlib.sha1(b'blob '+str(len(content)).encode()+bytes([0])+content).hexdigest()==blob
 with (root/name).open('xb') as output:output.write(content)
 files[name]={'source_url':url,'git_blob_sha1':blob,'bytes':len(content),'sha256':hashlib.sha256(content).hexdigest()}
with (root/'upstream-licenses.json').open('x') as out:json.dump(files,out,indent=2)
print(json.dumps({'source_commit':'cd1d6d5467322dbc0d39a80cb671d000ac302798','licenses':files}))
PY
'''
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=150)
Path('artifacts/advanced-split/receipts/delivery-04-neo-nextest-licenses.log').write_bytes(r.stdout+r.stderr)
print(r.stdout.decode());print(r.stderr.decode());raise SystemExit(r.returncode)
