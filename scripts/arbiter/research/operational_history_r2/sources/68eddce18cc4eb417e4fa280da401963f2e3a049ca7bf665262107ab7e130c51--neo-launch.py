# SPDX-License-Identifier: Apache-2.0
import json,shlex,subprocess
from pathlib import Path
base=Path('artifacts/readiness-1817');c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());remote='/Users/tylerjensen/airk-builds/readiness-1817-r1';py='/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools/python3.13'
s='''import json,os,subprocess
from pathlib import Path
p=Path('''+repr(remote)+'''); assert p.parent==Path.home()/'airk-builds'; assert not (p/'owner-launch.json').exists(); f=json.loads((p/'dependency-fetch.json').read_text()); assert f['status']=='passed'; out=(p/'owner.log').open('x'); child=subprocess.Popen(['''+repr(py)+''',str(p/'owner.py')],cwd=p,stdin=subprocess.DEVNULL,stdout=out,stderr=subprocess.STDOUT,start_new_session=True); r={'pid':child.pid,'source':f['native_source'],'owner':'exact newly created batch only'}; (p/'owner-launch.json').write_text(json.dumps(r)); print(json.dumps(r))'''
r=subprocess.run(['ssh','-i',c['identity_path'],*c['ssh_options'],c['user']+'@'+c['host'],shlex.quote(py)+' -c '+shlex.quote(s)],text=True,capture_output=True,timeout=30)
(base/'neo-launch.json').write_text(json.dumps({'exit_code':r.returncode,'stdout':r.stdout,'stderr':r.stderr,'command_scope':remote},indent=2)+'\n');print(r.stdout);print(r.stderr);raise SystemExit(r.returncode)
