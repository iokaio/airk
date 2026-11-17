# SPDX-License-Identifier: Apache-2.0
import hashlib,json,subprocess
from pathlib import Path
original=json.loads(Path('artifacts/advanced-split/mac-platform-source-plan.json').read_text());changed=json.loads(Path('artifacts/advanced-split/receipts/mac-format-r1/formatted-source.json').read_text())
assert original['source_set_sha256']=='7d913a63b20692b4b2520f4e1d2ac66e20f85a098c448b517d8aeeee48b868b6'
rows=original['source_files']
import base64
for name,row in changed['changed'].items():
 assert rows[name]['sha256']==row['before_sha256'];data=base64.b64decode(row['bytes']);rows[name]={'bytes':len(data),'sha256':row['after_sha256']}
seal=hashlib.sha256(json.dumps(rows,sort_keys=True,separators=(',',':')).encode()).hexdigest();plan={'schema':'airk.private-formatted-source/1','original_source_seal':original['source_set_sha256'],'source_set_sha256':seal,'format_only_files':list(changed['changed']),'files':rows,'preserved_extra_source':original['preserved_extra_source'],'scope':'actualroot cargo fmt --all only; selectednoinferenceCoreProcessIPCHost; originalsource/artifactreceipts retained'}
content=(json.dumps(plan,indent=2)+'\n').encode();path=Path('artifacts/advanced-split/receipts/mac-format-r1/formatted-source-seal.json');path.write_bytes(content);digest=hashlib.sha256(content).hexdigest()
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
remote_path='artifacts/advanced-split/receipts/mac-platform-format-r1-source.json'
r=subprocess.run(ssh+['sh','-c',"'cd \"$HOME/code/airk\" && test ! -e "+remote_path+" && cat > "+remote_path+"'"],input=content,capture_output=True,timeout=35)
if r.returncode:raise SystemExit(r.stderr.decode())
s=Path('artifacts/advanced-split/mac-platform-scoped-batch.sh').read_text();s=s.replace('mac-platform-7d913a63b20692b4','mac-platform-'+seal[:16]);a=s.index('source_check() {');b=s.index('\nsource_check\n',a)
check='''source_check() {
 "$platform_python" -I -B - <<'PY'
import hashlib,json
from pathlib import Path
source=Path('''+repr(remote_path)+''')
assert hashlib.sha256(source.read_bytes()).hexdigest()=='''+repr(digest)+'''
manifest=json.loads(source.read_bytes())
for name,row in manifest['files'].items():
 path=Path(name)
 assert path.is_file() and not path.is_symlink() and hashlib.sha256(path.read_bytes()).hexdigest()==row['sha256'],name
print('Exact formatted source seal verified',manifest['source_set_sha256'],len(manifest['files']))
PY
}'''
s=s[:a]+check+s[b:];s=s.replace('mac-platform-clippy "','mac-platform-clippy-attempt2 "').replace('mac-platform-tests cargo','mac-platform-tests-attempt2 cargo')
Path('artifacts/advanced-split/mac-platform-scoped-attempt2.sh').write_text(s,encoding='utf-8',newline='\n')
launcher=Path('artifacts/advanced-split/invoke-mac-platform-scoped.py').read_text().replace('mac-platform-scoped-batch.sh','mac-platform-scoped-attempt2.sh').replace('mac-platform-7d913a63b20692b4','mac-platform-'+seal[:16]).replace('mac-platform-scoped-batch.log','mac-platform-scoped-attempt2-launch.log')
Path('artifacts/advanced-split/invoke-mac-platform-attempt2.py').write_text(launcher,encoding='utf-8')
print(json.dumps({'formatted_source_seal':seal,'manifest_sha256':digest,'files':len(rows),'format_only_changes':len(changed['changed']),'helper':'python artifacts/advanced-split/invoke-mac-platform-attempt2.py'},indent=2))
