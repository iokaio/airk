# SPDX-License-Identifier: Apache-2.0
"""Read-only Mac R4 receipts/runtime authentication; no native execution."""
from pathlib import Path
import hashlib,json,subprocess,time
ROOT=Path.cwd();OUT=ROOT/'artifacts/advanced-split/receipts/mac-recovery-r131'
connection=json.loads((ROOT/'artifacts/v190/neo-connection.local.json').read_bytes())
SSH=['ssh','-p',str(connection['port']),'-i',connection['identity_path'],*connection['ssh_options'],connection['user']+'@'+connection['host'],'sh','-s']
program=r'''set -eu
/Users/tylerjensen/Code/airk/artifacts/tools/neo-python-3.13.18/bin/python3.13 -I -B - <<'PY'
from pathlib import Path
import hashlib,json,os,stat,struct,subprocess
base=Path('/Users/tylerjensen/airk-builds/advanced-split-local-delivery-r2');runtime=base/'artifacts/runtime-recovery-r4';receipts=base/'receipts/native-platform-r4';original=Path('/Users/tylerjensen/Code/airk')
assert os.geteuid()==501
for path in [base,runtime,receipts]:assert path.is_dir() and not path.is_symlink() and path.resolve()==path
sha=lambda p:hashlib.file_digest(p.open('rb'),'sha256').hexdigest()
files={}
for p in sorted(runtime.rglob('*')):
 assert not p.is_symlink()
 if not p.is_file():continue
 s=p.stat();assert stat.S_ISREG(s.st_mode) and s.st_nlink==1
 files[p.relative_to(runtime).as_posix()]={'bytes':s.st_size,'sha256':sha(p)}
assert len(files)<10000
build=json.loads((runtime/'airk-build.json').read_bytes());pair=json.loads((runtime/'airk-runtime.json').read_bytes());install=json.loads((runtime/'airk-install.json').read_bytes())
assert build['runtime']==pair and build['source']=={'sha256':'68d318d6ca248ba78de8c0ed6347d189574bd43770e8e76900440cc9e1d9927e','files':1855}
assert all(files[name]['sha256']==digest for name,digest in install['files'].items())
images={}
for name,digest in [('AIRK.app/Contents/MacOS/airk',pair['airk_sha256']),('AIRK.app/Contents/MacOS/airkctl',pair['airkctl_sha256'])]:
 assert files[name]['sha256']==digest
 with (runtime/name).open('rb') as stream:header=stream.read(16)
 magic,cpu,subtype,kind=struct.unpack('<IIII',header);assert magic==0xfeedfacf and cpu==0x100000c and kind==2
 images[name]={**files[name],'mach_o':{'magic':hex(magic),'cpu_type':hex(cpu),'file_type':kind}}
env=dict(os.environ,GIT_OPTIONAL_LOCKS='0')
head=subprocess.check_output(['/usr/bin/git','rev-parse','HEAD'],cwd=original,env=env,text=True).strip();status=subprocess.check_output(['/usr/bin/git','status','--porcelain=v1','--untracked-files=no'],cwd=original,env=env,text=True).strip()
original_state={'head':head,'index_sha256':sha(original/'.git/index'),'tracked_status_sha256':hashlib.sha256(status.encode()).hexdigest()}
assert original_state=={'head':'7ca445f8c56194243a99f314d693df22f5923197','index_sha256':'1fee8695db65ebfff52dc5ed652bddef144c7f6c293e5b4d122da80f3ddfcc64','tracked_status_sha256':'b20f5df827291264b5c18646b6c95aa98d175c729387702398b19ef2d0becb1a'}
result=json.loads((receipts/'result.json').read_bytes());assert result['status']=='passed_scoped_platform_candidate' and result['inputs_unchanged'] and result['cleanup']['exit_code']==0
receipt_files={}
for p in sorted(receipts.rglob('*')):
 if p.is_file():
  assert not p.is_symlink() and p.stat().st_nlink==1
  receipt_files[p.relative_to(receipts).as_posix()]={'bytes':p.stat().st_size,'sha256':sha(p)}
print(json.dumps({'schema':'airk.private-readonly-mac-r4-artifact-authentication/1','runtime_root':str(runtime),'receipt_root':str(receipts),'images':images,'runtime_files':files,'receipt_files':receipt_files,'runtime_metadata':pair,'install_metadata':install,'build_receipt':build,'batch_result':result,'original_checkout':original_state,'original_checkout_unchanged':True,'no_native_execution_or_remote_writes':True}))
PY
'''
(OUT/'read-only-inventory.sh').write_bytes(program.encode());started=time.monotonic();r=subprocess.run(SSH,input=program.encode(),capture_output=True)
(OUT/'read-only-inventory.stderr.log').write_bytes(r.stderr)
if r.returncode:
 (OUT/'read-only-inventory.failed.stdout').write_bytes(r.stdout);raise SystemExit(r.returncode)
value=json.loads(r.stdout);(OUT/'artifact-authentication.json').write_bytes((json.dumps(value,indent=2)+'\n').encode())
proof={'command':'existing normal-owner SSH -> POSIX sh -s, read-only metadata/hash inventory','program_sha256':hashlib.sha256(program.encode()).hexdigest(),'exit_code':r.returncode,'elapsed_seconds':time.monotonic()-started,'credentials_printed':False,'remote_writes':False}
(OUT/'read-only-inventory-command.json').write_bytes((json.dumps(proof,indent=2)+'\n').encode());print(json.dumps({'exit_code':0,'images':value['images'],'runtime_files':len(value['runtime_files']),'receipt_files':len(value['receipt_files']),'original_checkout_unchanged':True}))
