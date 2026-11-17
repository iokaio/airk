# SPDX-License-Identifier: Apache-2.0
# One read-only Neo observation/copy. Never invokes Cargo, process control or cleanup.
from pathlib import Path
import importlib.util,json,base64,subprocess,shlex,time,datetime,hashlib
ROOT=Path(r"D:\code\Github\airk");E=ROOT/'artifacts/v190/final-plan/exec-20261116T052942Z';OUT=Path(__file__).resolve().parent
s=importlib.util.spec_from_file_location('custody',E/'candidate-overlay-preparation-r1/candidate_overlay.py');h=importlib.util.module_from_spec(s);s.loader.exec_module(h);actor=h.identity();assert h.security(OUT)['owner_sid']==h.HUMAN
source=(OUT/'read-only-monitor.py').read_text()
source=source.replace("p=out/'logs'/(phase+'.log')","p=(out/'logs'/(phase+'.log')) if phase=='cargo-clean' else (out/'receipts'/('neo-f5-independent-'+phase+'-r1.log'))")
source=source.replace(' for name in names:', " for phase in ('workspace-clippy','workspace-tests','doctests'):\n  if (out/(phase+'-outer-result.json')).exists():\n   names.extend('receipts/neo-f5-independent-'+phase+'-r1'+suffix for suffix in ('-start.json','-runner.py','.log','-cargo-timing.html'))\n for name in names:")
compile(source,'read-only-remote-observer','exec')
c=json.loads((ROOT/'artifacts/v190/neo-connection.local.json').read_text());prefix=['ssh','-i',str(Path(c['identity_path']).expanduser()),'-p',str(c['port']),*c['ssh_options'],c['user']+'@'+c['host']]
py='/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13';runner='import base64;exec(compile(base64.b64decode('+repr(base64.b64encode(source.encode()).decode())+'),"readonly-batch-observer","exec"))';guard='set -eu; e=$(id -u); r=$(id -ru); h=$(stat -f %u /dev/console); [ "$e" = 501 ] && [ "$r" = 501 ] && [ "$h" = 501 ] && [ "$e" -ne 0 ] || exit 21; exec '+shlex.quote(py)+' -I -B -c '+shlex.quote(runner)
command=[*prefix,'/bin/sh -c '+shlex.quote(guard)];begin=time.monotonic();done=subprocess.run(command,cwd=ROOT,capture_output=True,timeout=60)
stamp=datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
if done.returncode:
 (OUT/('observer-failure-'+stamp+'.json')).write_text(json.dumps({'actor':actor,'command':command,'exit_code':done.returncode,'stderr':done.stderr.decode(errors='replace')},indent=2)+'\n',encoding='utf-8');raise SystemExit('Readonly SSH observer failed; retained; no Cargo replay.')
data=json.loads(done.stdout);assert data['uid']==data['effective_uid']==data['console_uid']==501
snapshot=OUT/('observation-'+stamp+'.json');snapshot.write_text(json.dumps({'utc':stamp,'actor':actor,'command':command,'elapsed_seconds':time.monotonic()-begin,'observation':data},indent=2)+'\n',encoding='utf-8')
summary={'utc':stamp,'identity':[data['uid'],data['effective_uid'],data['console_uid']],'target_exists':data['target_exists'],'phases':{},'logs':{}}
completed=[]
for name,encoded in data['files'].items():
 if name.endswith('-outer-result.json'):
  row=json.loads(base64.b64decode(encoded));result=row['result'];phase=row['phase'];completed.append(phase);summary['phases'][phase]={'passed':row['passed'],'exit_code':result.get('exit_code'),'elapsed_seconds':result.get('elapsed_seconds'),'settled':result.get('owned_descendants_settled'),'signaled':result.get('owned_group_signaled')}
  receiptname='receipts/neo-f5-independent-'+phase+'-r1.json'
  if receiptname in data['files']:
   receipt=json.loads(base64.b64decode(data['files'][receiptname]));summary['phases'][phase]['receipt']={k:receipt.get(k) for k in ('passed','exit_code','runner_exit_code','selected_tests','elapsed_seconds','inputs_unchanged','runner_unchanged','command_inputs_unchanged')}
for name in ('batch-result.json','cargo-clean-result.json'):
 if name in data['files']:summary[name]=json.loads(base64.b64decode(data['files'][name]))
for name,row in data['log_tails'].items():summary['logs'][name]={'bytes_total':row['bytes_total'],'last_lines':row['tail'].splitlines()[-5:]}
if completed:
 dest=OUT/('completed-phase-custody-'+stamp);dest.mkdir();copies=[]
 for name,encoded in data['files'].items():
  if name in ('batch-start.json','batch-result.json','cargo-clean-result.json') or any(name.startswith('receipts/neo-f5-independent-'+p+'-r1') or name in (p+'-outer-start.json',p+'-outer-result.json') for p in completed):
   p=dest/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(base64.b64decode(encoded));assert h.security(p)['owner_sid']==h.HUMAN;copies.append({'path':name,'sha256':h.sha(p)})
 (dest/'custody.json').write_text(json.dumps({'utc':stamp,'identity':summary['identity'],'observation':str(snapshot),'copies':copies,'scope':'Exact completed-phase receipt/log/timing/start/runner copies; no target/privatefixture copies or cleanup control.'},indent=2)+'\n',encoding='utf-8');summary['custody']=str(dest)
print(json.dumps(summary,indent=2))
