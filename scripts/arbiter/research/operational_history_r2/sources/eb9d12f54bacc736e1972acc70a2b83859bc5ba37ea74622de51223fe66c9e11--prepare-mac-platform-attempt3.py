# SPDX-License-Identifier: Apache-2.0
import base64,hashlib,json,subprocess
from pathlib import Path
old=json.loads(Path('artifacts/advanced-split/receipts/mac-format-r1/formatted-source-seal.json').read_text());rows=old['files'];names=['crates/airk-ipc/src/install_reader/history/tests.rs','fixtures/arbiter/cpu-fit-16.json'];changes={}
for name in names:
 data=Path(name).read_bytes();changes[name]={'before_sha256':rows.get(name,{}).get('sha256'),'after_sha256':hashlib.sha256(data).hexdigest(),'bytes':base64.b64encode(data).decode()};rows[name]={'bytes':len(data),'sha256':changes[name]['after_sha256']}
seal=hashlib.sha256(json.dumps(rows,sort_keys=True,separators=(',',':')).encode()).hexdigest();new={'schema':'airk.private-corrected-source/1','previous_source_seal':old['source_set_sha256'],'source_set_sha256':seal,'files':rows,'corrections':{n:{k:v for k,v in row.items() if k!='bytes'} for n,row in changes.items()},'scope':'Unix-testmutability only and authenticCPU16contractfixture transfer; no product/model/qualificationpolicy changes'}
payload=json.dumps({'changes':changes,'source':new},sort_keys=True).encode();content=(json.dumps(new,indent=2)+'\n').encode();manifest_sha=hashlib.sha256(content).hexdigest();newpath=Path('artifacts/advanced-split/receipts/mac-platform-correction-r1-source.json');newpath.write_bytes(content)
remote=r'''set -eu
cd "$HOME/code/airk"
/usr/bin/python3 - <<'PY'
import base64,hashlib,json,os,stat,tempfile
from pathlib import Path
payload=json.loads(base64.b64decode('__PAYLOAD__'));root=Path.cwd().resolve();stage=root/'artifacts/advanced-split/receipts/mac-platform-correction-r1';stage.mkdir()
for name,row in payload['changes'].items():
 path=root/name
 for parent in (path,*path.parents):
  if parent.is_symlink():raise ValueError('Aliased source path')
  if parent==root:break
 before=row['before_sha256']
 if before is None:
  if path.exists():raise ValueError('Unexpected existing fixture')
 else:
  assert path.is_file() and path.stat().st_nlink==1 and hashlib.sha256(path.read_bytes()).hexdigest()==before
  original=stage/'original'/name;original.parent.mkdir(parents=True,exist_ok=True)
  with original.open('xb') as out:out.write(path.read_bytes())
 with (stage/'request.json').open('x' if not (stage/'request.json').exists() else 'w') as out:json.dump({'scope':'exact2file corrective source request','changes':{n:{k:v for k,v in r.items() if k!='bytes'} for n,r in payload['changes'].items()}},out,indent=2)
for name,row in payload['changes'].items():
 path=root/name;data=base64.b64decode(row['bytes']);assert hashlib.sha256(data).hexdigest()==row['after_sha256'];path.parent.mkdir(parents=True,exist_ok=True)
 with tempfile.NamedTemporaryFile(dir=path.parent,prefix='.airk-source-correction-',delete=False) as out:out.write(data);temporary=Path(out.name)
 temporary.chmod(0o644);os.replace(temporary,path)
for name,row in payload['source']['files'].items():assert hashlib.sha256((root/name).read_bytes()).hexdigest()==row['sha256'],name
content=(json.dumps(payload['source'],indent=2)+'\n').encode()
with (root/'artifacts/advanced-split/receipts/mac-platform-correction-r1-source.json').open('xb') as out:out.write(content)
print(json.dumps({'source_set_sha256':payload['source']['source_set_sha256'],'verified_files':len(payload['source']['files']),'correction_files':list(payload['changes']),'recovery':str(stage),'source_manifest_sha256':hashlib.sha256(content).hexdigest()}))
PY
'''.replace('__PAYLOAD__',base64.b64encode(payload).decode())
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=45);Path('artifacts/advanced-split/receipts/mac-platform-correction-r1.log').write_bytes(r.stdout+r.stderr);print(r.stdout.decode());print(r.stderr.decode())
if r.returncode:raise SystemExit(r.returncode)
s=Path('artifacts/advanced-split/mac-platform-scoped-attempt2.sh').read_text().replace('mac-platform-ec3c3238ca7a52ae','mac-platform-'+seal[:16]).replace('mac-platform-format-r1-source.json','mac-platform-correction-r1-source.json').replace('ae11cb3581a8018cdde8404f0f447373208c7a3326c46abd391460795f1e608f',manifest_sha).replace('mac-platform-clippy-attempt2','mac-platform-clippy-attempt3').replace('mac-platform-tests-attempt2','mac-platform-tests-attempt3')
Path('artifacts/advanced-split/mac-platform-scoped-attempt3.sh').write_text(s,encoding='utf-8',newline='\n')
launcher=Path('artifacts/advanced-split/invoke-mac-platform-attempt2.py').read_text().replace('mac-platform-scoped-attempt2.sh','mac-platform-scoped-attempt3.sh').replace('mac-platform-ec3c3238ca7a52ae','mac-platform-'+seal[:16]).replace('mac-platform-scoped-attempt2-launch.log','mac-platform-scoped-attempt3-launch.log');Path('artifacts/advanced-split/invoke-mac-platform-attempt3.py').write_text(launcher,encoding='utf-8')
print('READY_HELPER python artifacts/advanced-split/invoke-mac-platform-attempt3.py')
