# SPDX-License-Identifier: Apache-2.0
# Root-only concrete transport. No action without an explicit phase argument.
from pathlib import Path
import argparse,base64,hashlib,importlib.util,io,json,os,shlex,subprocess,tarfile,time,datetime
ROOT=Path(r'D:\code\Github\airk');E=ROOT/'artifacts/v190/final-plan/exec-20261116T052942Z';B=Path(__file__).resolve().parent
PINS={'transfer/remote-source-bridge-r1.py':'bb72137d5f69df83bba7b26bbf2bc7d4cea45aacc6d90aee0c4c2c09038b6dbe','transfer/source-bridge-manifest.json':'3809404f750cdadd44ff9dc6e26646e07de07468182847fe28ef11f42512f37a','transfer/raw-source-delta.tar':'7a0d794ec984ebc111e3cb20b9471b52e1495db897460b69caea4eb9d482c154','transfer/f5f9dcec-to-59580fa6.bundle':'b70031219a2ebba6de5aa2787ec1b275dad363cf968dbaf2d97366d27c10430f','frozen-neo-config-r1.json':'c53b7d3aa891bec65014ed7a8d8e991c171cf2eb8a029849dadbc4a0662b606d','neo-next-integrated-platform-batch-r1.py':'24e04ee1cecadd6160adb814358d3338e47abe5d225c3736ba68e6b42ec16e08'}
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def frame(code,data):return str(len(code)).encode()+b'\n'+code+data
def short_bootstrap(digest):
 return "import sys,hashlib;h=sys.stdin.buffer.readline(16);assert h.endswith(b'\\n') and h[:-1].isdigit();n=int(h);assert 0<n<32768;c=sys.stdin.buffer.read(n);assert len(c)==n and hashlib.sha256(c).hexdigest()=="+repr(digest)+";exec(compile(c,'reviewed-stdin-owner','exec'))"
def main():
 parser=argparse.ArgumentParser();parser.add_argument('--phase',required=True,choices=['source','helper','launch']);args=parser.parse_args()
 spec=importlib.util.spec_from_file_location('existing_custody',E/'candidate-overlay-preparation-r1/candidate_overlay.py');h=importlib.util.module_from_spec(spec);spec.loader.exec_module(h);actor=h.identity();assert h.security(B)['owner_sid']==h.HUMAN
 for name,digest in PINS.items():h.regular(B/name);assert sha(B/name)==digest,name
 cfg=json.loads((B/'frozen-neo-config-r1.json').read_bytes());assert cfg['commit']=='59580fa6e6742e8101fd35346a2b999621671278' and cfg['raw_c7']=={'sha256':'e3356c7a0d6b94b109f6ad9a254a7df23806209fd6618a355278660ea53a78b9','files':1942}
 start_path=B/('Root-'+args.phase+'-start-r1.json');assert not start_path.exists(),'Prior attempt retained; no unchanged retry'
 connection=json.loads((ROOT/'artifacts/v190/neo-connection.local.json').read_bytes());prefix=['ssh','-i',str(Path(connection['identity_path']).expanduser()),'-p',str(connection['port']),*connection['ssh_options'],connection['user']+'@'+connection['host']]
 guard='set -eu; e=$(id -u); r=$(id -ru); h=$(stat -f %u /dev/console); [ "$e" = 501 ] && [ "$r" = 501 ] && [ "$h" = 501 ] && [ "$e" -ne 0 ] || exit 21; '
 data=b''
 if args.phase=='source':
  code=(B/'transfer/remote-source-bridge-r1.py').read_bytes();buffer=io.BytesIO()
  with tarfile.open(fileobj=buffer,mode='w:gz') as archive:
   for name in ['source-bridge-manifest.json','raw-source-delta.tar','f5f9dcec-to-59580fa6.bundle']:archive.add(B/'transfer'/name,arcname=name,recursive=False)
  data=frame(code,buffer.getvalue());remote=guard+'exec '+shlex.quote(cfg['python'])+' -I -B -c '+shlex.quote(short_bootstrap(hashlib.sha256(code).hexdigest()))
 elif args.phase=='helper':
  assert json.loads((B/'Root-source-result-r1.json').read_bytes())['passed'] is True
  code=(B/'helper-receive-stdin-r1.py').read_bytes();manifest=json.loads((B/'transport-preparation.json').read_bytes());assert sha(B/'helper-receive-stdin-r1.py')==manifest['helper_receiver_sha256']
  payload=json.dumps({name:base64.b64encode((B/name).read_bytes()).decode() for name in ['neo-next-integrated-platform-batch-r1.py','frozen-neo-config-r1.json']}).encode();data=frame(code,payload);remote=guard+'exec '+shlex.quote(cfg['python'])+' -I -B -c '+shlex.quote(short_bootstrap(hashlib.sha256(code).hexdigest()))
 else:
  assert json.loads((B/'Root-source-result-r1.json').read_bytes())['passed'] is True and json.loads((B/'Root-helper-result-r1.json').read_bytes())['passed'] is True
  launch=cfg['base']+'/evidence/platform-current-diagnostics-launch-r1';remote=guard+'export PYTHONDONTWRITEBYTECODE=1; exec '+shlex.quote(cfg['python'])+' -I -B '+shlex.quote(launch+'/neo-next-integrated-platform-batch-r1.py')+' '+shlex.quote(launch+'/frozen-neo-config-r1.json')
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
