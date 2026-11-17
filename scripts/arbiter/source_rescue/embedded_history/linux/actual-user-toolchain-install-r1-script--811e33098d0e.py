# SPDX-License-Identifier: Apache-2.0
import os,pwd,pathlib,json,subprocess,time,hashlib
assert os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=='tyler';os.umask(0o077)
B=pathlib.Path('/home/tyler/airk-builds/arbiter-final-linux-r1');assert B.resolve()==B and B.stat().st_uid==1000
source=B/'evidence/user-executable-acquisition-r1.json';assert hashlib.sha256(source.read_bytes()).hexdigest()=='397106e96691dd44d026f3325b09d63619baf5d3a03e386ad4f575227032cb8e';acq=json.loads(source.read_text());assert acq['passed']
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def homes():
 return {str(p):{'exists':p.exists(),'sha256':sha(p) if p.is_file() else None} for p in [pathlib.Path('/home/tyler')/n for n in ['.profile','.bashrc','.bash_profile','.zshrc','.cargo','.rustup']]}
record={'schema':'airk.private.linux-user-toolchain-install/1','uid':os.getuid(),'passed':False,'commands':[],'home_before':homes(),'cargo_build_test_install_invoked':False,'ort_or_native_model_invoked':False};out=B/'evidence/user-toolchain-install-r1.json';began=time.monotonic()
def save():out.write_text(json.dumps(record,indent=2)+'\n')
env={**os.environ,'HOME':'/home/tyler','CARGO_HOME':str(B/'cargo'),'RUSTUP_HOME':str(B/'rustup'),'TMPDIR':str(B/'tmp'),'PATH':str(B/'node/node-v24.15.0-linux-x64/bin')+':/usr/bin:/bin','PYTHONDONTWRITEBYTECODE':'1'}
def run(argv,timeout=30):
 argv=list(map(str,argv));t=time.monotonic();p=subprocess.run(argv,cwd=B,env=env,capture_output=True,text=True,timeout=timeout);record['commands'].append({'argv':argv,'exit_code':p.returncode,'seconds':time.monotonic()-t,'stdout':p.stdout,'stderr':p.stderr});save();assert p.returncode==0,record['commands'][-1];return p.stdout
try:
 for r in acq['downloads']:assert sha(pathlib.Path(r['path']))==r['sha256']
 assert run([B/'node/node-v24.15.0-linux-x64/bin/node','--version']).strip()=='v24.15.0'
 # Query nextest directly with Cargo's documented subcommand argument; no Cargo command.
 nextest=list((B/'nextest').rglob('cargo-nextest'));assert len(nextest)==1
 assert '0.9.148' in run([nextest[0],'nextest','--version'])
 run([B/'downloads/rustup-init','-y','--no-modify-path','--profile','minimal','--default-toolchain','1.98.0','--component','clippy','--component','rustfmt'],600)
 tools=B/'rustup/toolchains/1.98.0-x86_64-unknown-linux-gnu/bin';assert tools.is_dir()
 for name in ['rustc','cargo','rustfmt','clippy-driver']:assert '1.98' in run([tools/name,'--version'])
 record['tool_pins']=[{'path':str(p),'sha256':sha(p)} for p in [B/'node/node-v24.15.0-linux-x64/bin/node',nextest[0],*[tools/n for n in ['rustc','cargo','rustfmt','clippy-driver']]]]
 record['home_after']=homes();assert record['home_before']==record['home_after']
 for root in [B/'cargo',B/'rustup',B/'node',B/'nextest']:
  for p in root.rglob('*'):assert p.lstat().st_uid==1000 and p.resolve(strict=False).is_relative_to(B),str(p)
 record['passed']=True
except BaseException as e:record['failure']={'type':type(e).__name__,'message':str(e)};raise
finally:record['seconds']=time.monotonic()-began;save();print(json.dumps(record));print('RECEIPT_SHA256',sha(out))
