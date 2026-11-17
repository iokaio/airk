# SPDX-License-Identifier: Apache-2.0
import os,pwd,pathlib,json,hashlib,subprocess,tomllib
assert os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=='tyler';os.umask(0o077)
B=pathlib.Path('/home/tyler/airk-builds/arbiter-final-linux-r1');assert B.resolve()==B and B.stat().st_uid==1000
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
p=B/'evidence/user-toolchain-install-r1.json';old=json.loads(p.read_text());assert old['commands'][2]['exit_code']==0 and old['commands'][3]['stdout'].startswith('rustc 1.98.0') and old['commands'][4]['stdout'].startswith('cargo 1.98.0') and old['commands'][5]['stdout'].startswith('rustfmt 1.9.0-stable')
T=B/'rustup/toolchains/1.98.0-x86_64-unknown-linux-gnu';env={**os.environ,'CARGO_HOME':str(B/'cargo'),'RUSTUP_HOME':str(B/'rustup'),'PATH':str(T/'bin')+':'+str(B/'toolchain/bin')+':'+str(B/'node/node-v24.15.0-linux-x64/bin')+':/usr/bin:/bin','TMPDIR':str(B/'tmp')}
clippy=subprocess.run([str(T/'bin/clippy-driver'),'--version'],env=env,capture_output=True,text=True,timeout=20);assert clippy.returncode==0 and '0.1.98' in clippy.stdout
npm=subprocess.run([str(B/'node/node-v24.15.0-linux-x64/bin/node'),str(B/'node/node-v24.15.0-linux-x64/lib/node_modules/npm/bin/npm-cli.js'),'--version'],env=env,capture_output=True,text=True,timeout=20);assert npm.returncode==0
home_after={str(p):{'exists':p.exists(),'sha256':sha(p) if p.is_file() else None} for p in [pathlib.Path('/home/tyler')/n for n in ['.profile','.bashrc','.bash_profile','.zshrc','.cargo','.rustup']]};assert old['home_before']==home_after
counts={}
for root in [B/'cargo',B/'rustup',B/'node',B/'nextest']:
 n=0
 for p in root.rglob('*'):assert p.lstat().st_uid==1000 and p.resolve(strict=False).is_relative_to(B),str(p);n+=1
 counts[str(root)]=n
paths=[T/'bin'/n for n in ['rustc','cargo','rustfmt','clippy-driver','cargo-clippy']]+[B/'node/node-v24.15.0-linux-x64/bin/node',B/'nextest/cargo-nextest']
r={'schema':'airk.private.linux-user-toolchain-readiness/1','passed':True,'uid':os.getuid(),'prior_install_receipt':{'path':str(p),'sha256':sha(B/'evidence/user-toolchain-install-r1.json'),'passed':False},'prior_collector_correction':'rustfmt has independent component version1.9.0; Rust1.98 toolchain includes genuine rustfmt1.9 and clippy0.1.98. Installer was successful and is not re-executed.','actual_install_exit_code':old['commands'][2]['exit_code'],'versions_from_original_actual_observations':old['commands'][:2]+old['commands'][3:],'clippy_version':{'exit_code':clippy.returncode,'stdout':clippy.stdout,'stderr':clippy.stderr},'npm_version':npm.stdout.strip(),'home_profiles_unchanged':True,'home_after':home_after,'all_owned_output_counts':counts,'tool_pins':[{'path':str(q),'sha256':sha(q)} for q in paths],'child_environment':{k:env[k] for k in ['CARGO_HOME','RUSTUP_HOME','PATH','TMPDIR']},'cargo_build_test_or_install':False,'ORT_or_model_run':False}
q=B/'evidence/user-toolchain-readiness-r2.json';q.write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r));print('RECEIPT_SHA256',sha(q))
