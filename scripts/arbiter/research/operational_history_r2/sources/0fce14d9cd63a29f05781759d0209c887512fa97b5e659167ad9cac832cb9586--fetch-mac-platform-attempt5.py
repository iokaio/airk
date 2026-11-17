# SPDX-License-Identifier: Apache-2.0
import base64,json,subprocess
from pathlib import Path
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
remote='''set -eu
cd "$HOME/code/airk"
/usr/bin/python3 - <<'PY'
from pathlib import Path
import base64,json,subprocess
root=Path('artifacts/advanced-split/receipts/mac-platform-scoped');files={}
for p in sorted(root.rglob('*')):
 if not p.is_file() or p.is_symlink() or 'native-fixtures' in p.parts or not p.name.startswith(('mac-platform-clippy-attempt5','mac-platform-tests-attempt5')):continue
 if p.stat().st_size>4*1024*1024:raise ValueError('Unexpected large receipt')
 files[p.relative_to(root).as_posix()]=base64.b64encode(p.read_bytes()).decode()
r=subprocess.run(['/bin/ps','-axo','pid=,ppid=,comm=,args='],capture_output=True,text=True,check=True)
processes=[]
for line in r.stdout.splitlines():
 fields=line.split(None,3)
 if len(fields)>2 and Path(fields[2]).name in ('cargo','rustc','clippy-driver','rustfmt'):processes.append(line)
print(json.dumps({'files':files,'compiler_processes':processes}))
PY
'''
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=45)
if r.returncode:raise SystemExit(r.stderr.decode())
v=json.loads(r.stdout);out=Path('artifacts/advanced-split/receipts/mac-platform-scoped-attempt5');out.mkdir(exist_ok=False)
for name,content in v['files'].items():
 p=out/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(base64.b64decode(content))
(out/'compiler-exit-observation.json').write_text(json.dumps({'compiler_processes':v['compiler_processes']},indent=2))
print('RETAINED_FILES',list(v['files']));print('COMPILER_PROCESSES',v['compiler_processes'])
for name in ['mac-platform-clippy-attempt5.json','mac-platform-tests-attempt5.json']:
 p=out/name
 if p.exists():
  receipt=json.loads(p.read_text());print(name,json.dumps({k:receipt.get(k) for k in ['exit_code','elapsed_seconds','inputs_unchanged','selected_tests','test_summaries','cargo_timings','junit']}))
