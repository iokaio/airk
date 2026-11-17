# SPDX-License-Identifier: Apache-2.0
# ROOT ONLY: explicit flag authorizes the exact reviewed manual target cleanup.
from pathlib import Path
import argparse,datetime,hashlib,importlib.util,json,shlex,subprocess,time
R=Path(r'D:\code\Github\airk');E=R/'artifacts/v190/final-plan/exec-20261116T052942Z';B=Path(__file__).resolve().parent
REMOTE_SHA='1449f9bae10717b84f4636cd44a20e43f0db050c47a9dec2acb2ecade9cbbedc'
P=argparse.ArgumentParser();P.add_argument('--execute-root-reviewed-exact-target',action='store_true',required=True);P.parse_args()
s=importlib.util.spec_from_file_location('h',E/'candidate-overlay-preparation-r1/candidate_overlay.py');h=importlib.util.module_from_spec(s);s.loader.exec_module(h);actor=h.identity();assert h.security(B)['owner_sid']==h.HUMAN
code=(B/'manual-clean-remote.py').read_bytes();assert hashlib.sha256(code).hexdigest()==REMOTE_SHA
out=B/'actual-root-execution-r1';assert not out.exists(),'Preserve earlier attempt; no automatic cleanup replay';out.mkdir()
connection=json.loads((R/'artifacts/v190/neo-connection.local.json').read_bytes());prefix=['ssh','-i',str(Path(connection['identity_path']).expanduser()),'-p',str(connection['port']),*connection['ssh_options'],connection['user']+'@'+connection['host']]
python='/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13'
guard='set -eu; e=$(id -u); r=$(id -ru); h=$(stat -f %u /dev/console); [ "$e" = 501 ] && [ "$r" = 501 ] && [ "$h" = 501 ] && [ "$e" -ne 0 ] || exit 21; '
command=prefix+['/bin/sh -c '+shlex.quote(guard+'exec '+shlex.quote(python)+' -I -B - --execute-root-reviewed-exact-target')]
start=time.monotonic();initial={'utc':datetime.datetime.now(datetime.UTC).isoformat(),'actor':actor,'remote_helper_sha256':REMOTE_SHA,'target':'/Users/tylerjensen/airk-builds/arbiter-final-control-alias-e306258a-r1/source/target','action':'Exact user-authorized separate manual artifact cleanup; original batch remains nonpass. No Cargo/tests.'}
(out/'start.json').write_text(json.dumps(initial,indent=2)+'\n',encoding='utf-8')
try:
 with (out/'stdout.log').open('xb') as stdout,(out/'stderr.log').open('xb') as stderr:run=subprocess.run(command,input=code,stdout=stdout,stderr=stderr,cwd=R,timeout=None)
 result={'ssh_exit_code':run.returncode,'elapsed_seconds':time.monotonic()-start,'passed':run.returncode==0,'remote_result_must_be_retained':True}
except BaseException as failure:result={'passed':False,'error':{'type':type(failure).__name__,'message':str(failure)},'elapsed_seconds':time.monotonic()-start,'remote_state':'unknown; preserve evidence; no retry/signals/delete'}
(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result),flush=True)
if not result['passed']:raise SystemExit(1)
