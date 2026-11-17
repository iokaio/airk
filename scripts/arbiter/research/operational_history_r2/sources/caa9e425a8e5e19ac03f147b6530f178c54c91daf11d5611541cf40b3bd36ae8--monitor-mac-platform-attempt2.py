# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
remote='''set -eu
cd "$HOME/code/airk"
/usr/bin/python3 - <<'PY'
from pathlib import Path
import datetime,json,subprocess
root=Path('artifacts/advanced-split/receipts/mac-platform-scoped')
for name in ['mac-platform-clippy-attempt2.log','mac-platform-clippy-attempt2.json','mac-platform-tests-attempt2.log','mac-platform-tests-attempt2.json']:
 path=root/name
 if not path.is_file():continue
 if path.suffix=='.json':
  v=json.loads(path.read_text());print('RECEIPT',name,json.dumps({k:v.get(k) for k in ['exit_code','elapsed_seconds','inputs_unchanged','selected_tests','test_summaries','changed_groups']}))
 else:
  with path.open('rb') as stream:stream.seek(max(0,path.stat().st_size-7000));value=stream.read()
  print('LOG',name,'BYTES',path.stat().st_size,'MODIFIED',path.stat().st_mtime);print(value.decode(errors='replace'))
result=subprocess.run(['/bin/ps','-axo','pid=,ppid=,etime=,comm=,args='],capture_output=True,text=True,check=True)
for line in result.stdout.splitlines():
 parts=line.split(None,4)
 if len(parts)>3 and Path(parts[3]).name in ('cargo','rustc','clippy-driver','rustfmt','cc','c++','clang','clang++'):print('BUILD_PROCESS',line)
print('OBSERVED_UTC',datetime.datetime.now(datetime.timezone.utc).isoformat())
PY
'''
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=45)
from datetime import datetime,timezone
stamp=datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ');out=Path('artifacts/advanced-split/receipts')/('mac-platform-attempt2-progress-'+stamp+'.log');out.write_bytes(r.stdout+r.stderr)
print(r.stdout.decode(errors='replace'));print(r.stderr.decode(errors='replace'));raise SystemExit(r.returncode)
