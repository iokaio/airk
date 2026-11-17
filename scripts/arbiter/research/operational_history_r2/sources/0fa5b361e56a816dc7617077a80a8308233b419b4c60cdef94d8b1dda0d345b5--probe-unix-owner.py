# SPDX-License-Identifier: Apache-2.0
import base64,json,shlex,subprocess
from pathlib import Path
config=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());assert config['host']=='192.168.68.61'
code='''
import errno,json,os,subprocess,time,signal
result={'purpose':'harmless owned true process tests Darwin waitid and group cleanup; no compiler or AIRK','calls':[]}
p=subprocess.Popen(['/usr/bin/true'],start_new_session=True)
result['owned_pid']=p.pid
time.sleep(.2)
try:
 value=os.waitid(os.P_PID,p.pid,os.WEXITED|os.WNOHANG|os.WNOWAIT)
 result['calls'].append({'syscall':'waitid WNOWAIT','result':list(value) if value else None})
except OSError as e:result['calls'].append({'syscall':'waitid WNOWAIT','errno':e.errno,'error':e.strerror})
try:
 os.killpg(p.pid,signal.SIGTERM);result['calls'].append({'syscall':'killpg SIGTERM owned unreaped true','result':'success'})
except OSError as e:result['calls'].append({'syscall':'killpg SIGTERM owned unreaped true','errno':e.errno,'error':e.strerror})
result['wait_exit']=p.wait(timeout=5)
try:
 os.killpg(p.pid,0);result['calls'].append({'syscall':'killpg 0 after reap','result':'exists'})
except OSError as e:result['calls'].append({'syscall':'killpg 0 after reap','errno':e.errno,'error':e.strerror})
print(json.dumps(result))
'''
python='/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13'
command=shlex.quote(python)+' -c '+shlex.quote('import base64;exec(base64.b64decode('+repr(base64.b64encode(code.encode()).decode())+'))')
p=subprocess.run(['ssh','-i',config['identity_path'],*config['ssh_options'],config['user']+'@'+config['host'],command],capture_output=True,timeout=30)
out=Path('artifacts/local/one-airk-1814/neo-static-ort/supplement-1794092444')
(out/'unix-owner-probe.stdout.json').write_bytes(p.stdout);(out/'unix-owner-probe.stderr').write_bytes(p.stderr)
print(p.stdout.decode());print(p.stderr.decode());raise SystemExit(p.returncode)
