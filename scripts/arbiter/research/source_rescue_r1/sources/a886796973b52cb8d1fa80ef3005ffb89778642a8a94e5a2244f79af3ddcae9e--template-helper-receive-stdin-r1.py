# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import os,sys,json,stat,base64,hashlib,subprocess
assert os.getuid()==os.geteuid()==501==os.stat('/dev/console').st_uid and os.getuid()!=0
base=Path('/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1');parent=base/'evidence';dest=parent/'platform-current-diagnostics-launch-r1'
for p in (base,parent):
 s=p.lstat();assert stat.S_ISDIR(s.st_mode) and s.st_uid==501 and p.resolve()==p and not p.is_symlink()
assert not dest.exists() and not (base/'source/target').exists()
assert subprocess.check_output(['git','--no-optional-locks','rev-parse','HEAD'],cwd=base/'source').decode().strip()=='59580fa6e6742e8101fd35346a2b999621671278'
assert not subprocess.check_output(['git','--no-optional-locks','status','--porcelain=v1','--untracked-files=all'],cwd=base/'source')
expected={'neo-next-integrated-platform-batch-r1.py':('24e04ee1cecadd6160adb814358d3338e47abe5d225c3736ba68e6b42ec16e08',10297),'frozen-neo-config-r1.json':('c53b7d3aa891bec65014ed7a8d8e991c171cf2eb8a029849dadbc4a0662b606d',6090)}
raw=sys.stdin.buffer.read(65537);assert len(raw)<=65536
received=json.loads(raw);assert set(received)==set(expected)
contents={name:base64.b64decode(value,validate=True) for name,value in received.items()}
for name,data in contents.items():assert len(data)==expected[name][1] and hashlib.sha256(data).hexdigest()==expected[name][0]
os.umask(0o077);dest.mkdir(mode=0o700)
for name,data in contents.items():
 p=dest/name
 with p.open('xb') as f:f.write(data);f.flush();os.fsync(f.fileno())
 s=p.lstat();assert stat.S_ISREG(s.st_mode) and s.st_uid==501 and s.st_nlink==1 and hashlib.sha256(p.read_bytes()).hexdigest()==expected[name][0]
print(json.dumps({'uid':os.getuid(),'effective_uid':os.geteuid(),'console_uid':os.stat('/dev/console').st_uid,'helper_dir':str(dest),'sha256':{name:value[0] for name,value in expected.items()}}))
