# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
remote='''set -eu
cd "$HOME/code/airk"
PYTHONDONTWRITEBYTECODE=1 artifacts/tools/neo-python-3.13.18/bin/python3.13 -I -B - <<'PY'
import hashlib,importlib.util,json,os,re,tarfile,time,urllib.request
from pathlib import Path,PurePosixPath
root=Path('artifacts/tools/neo-nextest-0.9.148').resolve()
if root.exists():raise ValueError('Existing Nextest tool evidence retained; no overwrite')
root.mkdir(parents=True)
base='https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-0.9.148/'
name='cargo-nextest-0.9.148-universal-apple-darwin.tar.gz';expected='6c23b7fb4ca82c571fc8dfedbab45bfa773d45918f933170c85d21cd7f4b852b'
for filename,size,sha in [(name,16758789,expected),('cargo-nextest-0.9.148-universal-apple-darwin.sha256',118,'19eacd16f7442d18840b431e62dbb9f88940f91403045b98002d9117de3a980e')]:
 target=root/filename;digest=hashlib.sha256();total=0;start=time.monotonic()
 with urllib.request.urlopen(base+filename,timeout=15) as source,target.open('xb') as out:
  while True:
   chunk=source.read(1024*1024)
   if not chunk:break
   total+=len(chunk)
   if total>size or time.monotonic()-start>90:raise ValueError('Tool acquisition bound')
   digest.update(chunk);out.write(chunk)
 if total!=size or digest.hexdigest()!=sha:raise ValueError('Official tool asset differs')
checksum=(root/'cargo-nextest-0.9.148-universal-apple-darwin.sha256').read_text()
if not re.fullmatch(expected+r'\s+\\*?'+re.escape(name)+r'\s*',checksum):raise ValueError('Official checksum text differs')
rows={};total=0
with tarfile.open(root/name,'r:gz') as source:
 for member in source.getmembers():
  path=PurePosixPath(member.name)
  if path.is_absolute() or any(p in ('..','.','') for p in path.parts) or len(path.parts)>4 or not member.isfile() or member.name in rows or member.size>100*1024*1024 or member.mode&0o7000:raise ValueError('Unapproved Nextest archive member')
  total+=member.size
  if total>128*1024*1024 or len(rows)>100:raise ValueError('Nextest extraction bound')
  content=source.extractfile(member).read();rows[member.name]={'bytes':len(content),'sha256':hashlib.sha256(content).hexdigest(),'mode':member.mode&0o777}
  output=root/member.name;output.parent.mkdir(parents=True,exist_ok=True)
  with output.open('xb') as out:out.write(content)
  output.chmod(member.mode&0o777)
record={'version':'0.9.148','upstream':'nextest-rs/nextest','tag':'cargo-nextest-0.9.148','asset_id':620608067,'archive_sha256':expected,'checksum_asset_sha256':'19eacd16f7442d18840b431e62dbb9f88940f91403045b98002d9117de3a980e','files':rows}
(root/'provision-receipt.json').write_text(json.dumps(record,indent=2))
s=importlib.util.spec_from_file_location('core',Path('scripts/provision-static-onnx-runtime.py'));core=importlib.util.module_from_spec(s);s.loader.exec_module(core)
log=Path('artifacts/advanced-split/receipts/neo-nextest-0.9.148-version.log').resolve();r=core.run_owned([str(root/'cargo-nextest'),'nextest','--version'],root,log,20)
core.write_json(log.with_suffix('.json'),r)
assert core.successful(r)
assert re.search(r'cargo-nextest 0\\.9\\.148(?:\\s|$)',log.read_text())
print(json.dumps({'root':str(root),'files':rows,'version_output':log.read_text(),'owned_probe':r,'provision_receipt_sha256':hashlib.sha256((root/'provision-receipt.json').read_bytes()).hexdigest()}))
PY
'''
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=150)
Path('artifacts/advanced-split/receipts/delivery-04-neo-nextest-provision.log').write_bytes(r.stdout+r.stderr)
print(r.stdout.decode());print(r.stderr.decode());raise SystemExit(r.returncode)
