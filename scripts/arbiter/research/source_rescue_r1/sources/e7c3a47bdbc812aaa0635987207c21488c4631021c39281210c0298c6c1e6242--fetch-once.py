# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import base64,datetime,hashlib,json,os,shlex,subprocess,time
ROOT=Path(r'D:\code\Github\airk');OUT=Path(__file__).resolve().parent;SOURCE=OUT.parent/'bounded-ENOENT-fixture-census-PROPOSAL.py'
source=SOURCE.read_bytes();expected=__import__('sys').argv[1];assert hashlib.sha256(source).hexdigest()==expected
assert subprocess.check_output(['whoami','/user','/fo','csv','/nh'],text=True).find('S-1-5-21-3678370870-3858642417-826968105-1002')>=0
config=json.loads((ROOT/'artifacts/v190/neo-connection.local.json').read_text());prefix=['ssh','-i',str(Path(config['identity_path']).expanduser()),'-p',str(config['port']),*config['ssh_options'],config['user']+'@'+config['host']]
python='/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13'
runner='import base64;exec(compile(base64.b64decode('+repr(base64.b64encode(source).decode())+'),"readonly-retained-fixture-census","exec"))'
guard='set -eu; e=$(id -u); r=$(id -ru); h=$(stat -f %u /dev/console); [ "$e" = 501 ] && [ "$r" = 501 ] && [ "$h" = 501 ] && [ "$e" -ne 0 ] || exit 21; exec '+shlex.quote(python)+' -I -B -c '+shlex.quote(runner)
command=[*prefix,'/bin/sh -c '+shlex.quote(guard)];start=time.monotonic();utc=datetime.datetime.now(datetime.UTC).isoformat()
with (OUT/'start.json').open('x',encoding='utf-8') as f:json.dump({'utc':utc,'source_sha256':expected,'command':command,'timeout_seconds':60,'scope':'One guarded read-only retained-fixture evidence fetch; no native/Cargo/process-control operations.'},f,indent=2)
try:
 done=subprocess.run(command,cwd=ROOT,capture_output=True,timeout=60)
except BaseException as error:
 with (OUT/'failure.json').open('x',encoding='utf-8') as f:json.dump({'error':str(error),'elapsed_seconds':time.monotonic()-start,'retry':False},f)
 raise
for name,data in [('response.jsonl',done.stdout),('stderr.txt',done.stderr)]:
 with (OUT/name).open('xb') as f:f.write(data)
receipt={'utc':utc,'source_sha256':expected,'exit_code':done.returncode,'elapsed_seconds':time.monotonic()-start,'stdout_sha256':hashlib.sha256(done.stdout).hexdigest(),'stdout_bytes':len(done.stdout),'stderr_sha256':hashlib.sha256(done.stderr).hexdigest(),'stderr_bytes':len(done.stderr),'retry':False}
with (OUT/'result.json').open('x',encoding='utf-8') as f:json.dump(receipt,f,indent=2)
assert done.returncode==0,'Read-only fetch failed; preserve raw response/stderr and do not retry'
rows=[json.loads(line) for line in done.stdout.splitlines()];assert len(rows)==2 and rows[1]['identity']=={'real_uid':501,'effective_uid':501,'console_uid':501}
print(json.dumps({'passed':True,'elapsed_seconds':receipt['elapsed_seconds'],'stdout_bytes':len(done.stdout),'identity':rows[1]['identity'],'streams':len(rows[1]['streams_order_by_name_then_fixed_metadata']),'receipt':str(OUT/'result.json')}))
