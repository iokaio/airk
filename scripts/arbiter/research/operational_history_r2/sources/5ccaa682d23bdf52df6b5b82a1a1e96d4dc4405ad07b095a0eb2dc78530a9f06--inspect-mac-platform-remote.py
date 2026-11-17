# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
remote='''set -eu
cd "$HOME/code/airk"
/usr/bin/python3 - <<'PY'
import json,hashlib,subprocess
from pathlib import Path
root=Path.cwd().resolve();names=subprocess.check_output(['git','-c','safe.directory='+str(root),'ls-files','--cached','--others','--exclude-standard','--','crates'],text=True).splitlines();values={}
for name in names:
 p=Path(name)
 if p.is_file() and not p.is_symlink():values[name]=hashlib.sha256(p.read_bytes()).hexdigest()
status=subprocess.check_output(['git','-c','safe.directory='+str(root),'status','--short','--','crates','Cargo.toml','Cargo.lock','rust-toolchain.toml','scripts'],text=True)
print(json.dumps({'source_sha256':values,'source_status':status}))
PY
'''
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=45)
if r.returncode:raise SystemExit(r.stderr.decode())
v=json.loads(r.stdout);Path('artifacts/advanced-split/receipts/mac-platform-remote-before.json').write_text(json.dumps(v,indent=2)+'\n',encoding='utf-8');plan=json.loads(Path('artifacts/advanced-split/mac-platform-source-plan.json').read_text());extras=[n for n in v['source_sha256'] if n not in plan['source_files'] and n.endswith(('.rs','Cargo.toml'))];print(json.dumps({'remote_source_files':len(v['source_sha256']),'extra_native_source_requiring_review':extras,'remote_dirty_status_retained':True},indent=2))
