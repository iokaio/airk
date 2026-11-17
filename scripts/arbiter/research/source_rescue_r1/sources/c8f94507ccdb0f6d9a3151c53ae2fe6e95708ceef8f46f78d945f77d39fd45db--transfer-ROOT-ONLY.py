# SPDX-License-Identifier: Apache-2.0
# Root-only concrete transport. No action without an explicit phase argument.
from pathlib import Path
import argparse,base64,hashlib,importlib.util,io,json,os,shlex,subprocess,tarfile,time,datetime
ROOT=Path(r'D:\code\Github\airk');E=ROOT/'artifacts/v190/final-plan/exec-20261116T052942Z';B=Path(__file__).resolve().parent
PINS={'batch-controller.py': '191debc8cacb6207a782bf3236afa39fa02ab107dcd2326f47cc0e1da0387058', 'frozen-config.json': '890ab242dbeb5157a0f90af6952bfd61e2d923ab1b0b2f4bfd6c930813751a4a', 'receive-helper.py': '0708d6468a55699d73105b08773cbcc8c9525648c3197cb16f85562c2b21da80'}
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def frame(code,data):return str(len(code)).encode()+b'\n'+code+data
def short_bootstrap(digest):
 return "import sys,hashlib;h=sys.stdin.buffer.readline(16);assert h.endswith(b'\\n') and h[:-1].isdigit();n=int(h);assert 0<n<32768;c=sys.stdin.buffer.read(n);assert len(c)==n and hashlib.sha256(c).hexdigest()=="+repr(digest)+";exec(compile(c,'reviewed-stdin-owner','exec'))"
def main():
 parser=argparse.ArgumentParser();parser.add_argument('--phase',required=True,choices=['helper','launch']);args=parser.parse_args()
 spec=importlib.util.spec_from_file_location('existing_custody',E/'candidate-overlay-preparation-r1/candidate_overlay.py');h=importlib.util.module_from_spec(spec);spec.loader.exec_module(h);actor=h.identity();assert h.security(B)['owner_sid']==h.HUMAN
 for name,digest in PINS.items():h.regular(B/name);assert sha(B/name)==digest,name
 cfg=json.loads((B/'frozen-config.json').read_bytes());assert cfg['commit']=='059a2a893069f89e8ea8aefec7f9bdb8bb6063b6' and cfg['raw_c7']=={'sha256': 'af3e344079146f27bd02ab7f7b379e82ae85112975f4a6219d01f26f74caa958', 'files': 1949}
 start_path=B/('Root-'+args.phase+'-start-r1.json');assert not start_path.exists(),'Prior attempt retained; no unchanged retry'
 connection=json.loads((ROOT/'artifacts/v190/neo-connection.local.json').read_bytes());prefix=['ssh','-i',str(Path(connection['identity_path']).expanduser()),'-p',str(connection['port']),*connection['ssh_options'],connection['user']+'@'+connection['host']]
 guard='set -eu; e=$(id -u); r=$(id -ru); h=$(stat -f %u /dev/console); [ "$e" = 501 ] && [ "$r" = 501 ] && [ "$h" = 501 ] && [ "$e" -ne 0 ] || exit 21; '
 data=b''
 if args.phase=='helper':
  code=(B/'receive-helper.py').read_bytes();payload=json.dumps({name:base64.b64encode((B/name).read_bytes()).decode() for name in ['batch-controller.py','frozen-config.json']}).encode()
  data=frame(code,payload);remote=guard+'exec '+shlex.quote(cfg['python'])+' -I -B -c '+shlex.quote(short_bootstrap(hashlib.sha256(code).hexdigest()))
 else:
  assert json.loads((B/'Root-helper-result-r1.json').read_bytes())['passed'] is True
  launch=cfg['base']+'/evidence/platform-system-control-alias-nextest-launch-r1';remote=guard+'export PYTHONDONTWRITEBYTECODE=1; exec '+shlex.quote(cfg['python'])+' -I -B '+shlex.quote(launch+'/batch-controller.py')+' '+shlex.quote(launch+'/frozen-config.json')
 command=[*prefix,'/bin/sh -c '+shlex.quote(remote)];assert len(subprocess.list2cmdline(command))<8000,'Windows command bound: code/archive must remain in stdin'
 started=time.monotonic();initial={'utc':datetime.datetime.now(datetime.UTC).isoformat(),'actor':actor,'phase':args.phase,'command':command,'stdin_bytes':len(data),'stdin_sha256':hashlib.sha256(data).hexdigest(),'native_or_Cargo_on_Windows':False}
 with start_path.open('x',encoding='utf-8') as f:json.dump(initial,f,indent=2)
 print(json.dumps({'phase':args.phase,'status':'started','ssh_argv_characters':len(subprocess.list2cmdline(command))}),flush=True)
 stdout=B/('Root-'+args.phase+'-stdout-r1.log');stderr=B/('Root-'+args.phase+'-stderr-r1.log')
 try:
  with stdout.open('xb') as out,stderr.open('xb') as err:done=subprocess.run(command,input=data,cwd=ROOT,stdout=out,stderr=err,timeout=None if args.phase=='launch' else 600)
  result={'phase':args.phase,'exit_code':done.returncode,'elapsed_seconds':time.monotonic()-started,'passed':done.returncode==0,'stdout_sha256':sha(stdout),'stderr_sha256':sha(stderr),'scope':'Exact guarded source/helper transfer' if args.phase!='launch' else 'Actual independent Mac diagnostics; each true native phase outcome remains in remote receipts; no retry or inferred success'}
 except BaseException as error:result={'phase':args.phase,'passed':False,'error':str(error),'elapsed_seconds':time.monotonic()-started,'remote_state':'unknown; preserve observations, no replay'}
 with (B/('Root-'+args.phase+'-result-r1.json')).open('x',encoding='utf-8') as f:json.dump(result,f,indent=2)
 print(json.dumps(result),flush=True)
 if not result['passed']:raise SystemExit(1)
if __name__=='__main__':main()
