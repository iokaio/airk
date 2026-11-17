# SPDX-License-Identifier: Apache-2.0
import hashlib,json,subprocess
from pathlib import Path
root=Path.cwd();b=root/'artifacts/readiness-1817';out=b/'neo-receipts';out.mkdir()
c=json.loads((root/'artifacts/v190/neo-connection.local.json').read_text());peer=c['user']+'@'+c['host'];remote='/Users/tylerjensen/airk-builds/readiness-1817-r1'
for pattern in ('*.json','*.log'):
 subprocess.run(['scp','-i',c['identity_path'],*c['ssh_options'],peer+':'+remote+'/'+pattern,str(out)],check=True,timeout=60)
subprocess.run(['scp','-r','-i',c['identity_path'],*c['ssh_options'],peer+':'+remote+'/cargo-timings',str(out)],check=True,timeout=60)
v=json.loads((out/'qualification.json').read_text());cl=json.loads((out/'cleanup.json').read_text());assert v['status']=='passed' and cl['group_absent'] and cl['cargo_clean_exit_code']==0 and cl['target_removed']
t=json.loads((out/'timings.json').read_text());assert t['files'] and not t['error']
for item in t['files']:
 with (out/'cargo-timings'/item['name']).open('rb') as f:assert hashlib.file_digest(f,'sha256').hexdigest()==item['sha256']
r={'status':'passed','native_source':v['native_source'],'steps':[{'name':s['name'],'seconds':s['seconds'],'selected_tests':s['selected_tests'],'exit_code':s['exit_code']} for s in v['steps']],'total_selected':sum(s['selected_tests'] or 0 for s in v['steps']),'retained_timing_reports':len(t['files']),'cleanup':cl}
(b/'neo-collected.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r))
