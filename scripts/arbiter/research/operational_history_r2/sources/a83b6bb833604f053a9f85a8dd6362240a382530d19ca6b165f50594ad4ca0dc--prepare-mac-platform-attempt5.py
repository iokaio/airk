# SPDX-License-Identifier: Apache-2.0
import base64,hashlib,json,subprocess
from pathlib import Path
names=['crates/airk-process/src/monotonic.rs','crates/airk-process/src/monotonic/CONTRACT.md','crates/airk-host/src/read_attempt.rs','crates/airk-host/src/read_attempt/tests.rs','crates/airk-host/src/read_attempt/CONTRACT.md','scripts/dev/receipt_runner.py','scripts/tests/test_receipt_runner.py','packages/contracts-gen/arbiter.schema.json','crates/airk-core/src/arbiter/mod.rs','crates/airk-core/src/arbiter/tests.rs','crates/airk-core/src/arbiter/CONTRACT.md','fixtures/arbiter/cpu-fit-24.json']
review=json.loads(Path('artifacts/advanced-split/receipts/mac-clock-domain-source-correction.json').read_text())
for row in review['source_files']:
 if row['path'] in names:assert hashlib.sha256(Path(row['path']).read_bytes()).hexdigest()==row['sha256'],row['path']
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
remote="""set -eu
cd "$HOME/code/airk"
/usr/bin/python3 - <<'PY'
import base64,hashlib,json
from pathlib import Path
names=__NAMES__
result={}
for name in names:
 path=Path(name)
 if path.exists():
  assert path.is_file() and not path.is_symlink();data=path.read_bytes();result[name]={'sha256':hashlib.sha256(data).hexdigest(),'content':base64.b64encode(data).decode()}
 else:result[name]=None
print(json.dumps(result))
PY
""".replace('__NAMES__',repr(names))
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=35)
if r.returncode:raise SystemExit(r.stderr.decode())
remote_before=json.loads(r.stdout);Path('artifacts/advanced-split/receipts/mac-platform-correction-r3-before.json').write_bytes(r.stdout)
old=json.loads(Path('artifacts/advanced-split/receipts/mac-platform-correction-r2-source.json').read_text());rows=old['files'];changes={}
for name in names:
 before=remote_before[name]
 if name in rows:assert before is not None and before['sha256']==rows[name]['sha256'],name
 data=Path(name).read_bytes();sha=hashlib.sha256(data).hexdigest();changes[name]={'before_sha256':before['sha256'] if before else None,'after_sha256':sha,'content':base64.b64encode(data).decode()};rows[name]={'bytes':len(data),'sha256':sha}
seal=hashlib.sha256(json.dumps(rows,sort_keys=True,separators=(',',':')).encode()).hexdigest();source={'schema':'airk.private-combined-source/1','previous_source_seal':old['source_set_sha256'],'source_set_sha256':seal,'files':rows,'correction_paths':names,'scope':'approvedAppleclockdomain+exactdeadline seam; privateTMPDIR harness; Rustcanonicalgolden; CPU24contract; no modelpayload/guardweakening'}
content=(json.dumps(source,indent=2,sort_keys=True)+'\n').encode();manifest_sha=hashlib.sha256(content).hexdigest();Path('artifacts/advanced-split/receipts/mac-platform-correction-r3-source.json').write_bytes(content)
payload=base64.b64encode(json.dumps({'changes':changes,'source':source}).encode()).decode()
remote=r'''set -eu
cd "$HOME/code/airk"
/usr/bin/python3 - <<'PY'
import base64,hashlib,json,os,stat,tempfile
from pathlib import Path
payload=json.loads(base64.b64decode('__PAYLOAD__'));root=Path.cwd().resolve();stage=root/'artifacts/advanced-split/receipts/mac-platform-correction-r3';stage.mkdir()
request={name:{key:value for key,value in row.items() if key!='content'} for name,row in payload['changes'].items()}
with (stage/'request.json').open('x') as out:json.dump(request,out,indent=2)
for name,row in payload['changes'].items():
 path=root/name
 for parent in (path,*path.parents):
  if parent.is_symlink():raise ValueError('Source alias')
  if parent==root:break
 if row['before_sha256'] is None:assert not path.exists(),name
 else:
  assert path.is_file() and path.stat().st_nlink==1 and hashlib.sha256(path.read_bytes()).hexdigest()==row['before_sha256'],name
  backup=stage/'original'/name;backup.parent.mkdir(parents=True,exist_ok=True)
  with backup.open('xb') as out:out.write(path.read_bytes())
for name,row in payload['changes'].items():
 path=root/name;data=base64.b64decode(row['content']);assert hashlib.sha256(data).hexdigest()==row['after_sha256'];path.parent.mkdir(parents=True,exist_ok=True)
 with tempfile.NamedTemporaryFile(dir=path.parent,prefix='.airk-source-correction-',delete=False) as out:out.write(data);temporary=Path(out.name)
 temporary.chmod(0o644);os.replace(temporary,path)
for name,row in payload['source']['files'].items():assert hashlib.sha256((root/name).read_bytes()).hexdigest()==row['sha256'],name
content=(json.dumps(payload['source'],indent=2,sort_keys=True)+'\n').encode()
with (root/'artifacts/advanced-split/receipts/mac-platform-correction-r3-source.json').open('xb') as out:out.write(content)
print(json.dumps({'source_set_sha256':payload['source']['source_set_sha256'],'verified_files':len(payload['source']['files']),'corrections':list(payload['changes']),'manifest_sha256':hashlib.sha256(content).hexdigest(),'recovery':str(stage)}))
PY
'''.replace('__PAYLOAD__',payload)
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=45);Path('artifacts/advanced-split/receipts/mac-platform-correction-r3.log').write_bytes(r.stdout+r.stderr);print(r.stdout.decode());print(r.stderr.decode())
if r.returncode:raise SystemExit(r.returncode)
s=Path('artifacts/advanced-split/mac-platform-scoped-attempt4.sh').read_text().replace('mac-platform-be56d153c8e43fdd','mac-platform-'+seal[:16]).replace('mac-platform-correction-r2-source.json','mac-platform-correction-r3-source.json').replace('80533024553dcf68dcf3f5f37b6f340d2583d4019eb2739c733eec0b0fb8a195',manifest_sha).replace('attempt4','attempt5')
Path('artifacts/advanced-split/mac-platform-scoped-attempt5.sh').write_text(s,encoding='utf-8',newline='\n')
launcher=Path('artifacts/advanced-split/invoke-mac-platform-attempt4.py').read_text().replace('attempt4','attempt5').replace('mac-platform-be56d153c8e43fdd','mac-platform-'+seal[:16]);Path('artifacts/advanced-split/invoke-mac-platform-attempt5.py').write_text(launcher,encoding='utf-8')
print('READY_HELPER python artifacts/advanced-split/invoke-mac-platform-attempt5.py');print('SOURCE_MANIFEST_SHA256',manifest_sha)
