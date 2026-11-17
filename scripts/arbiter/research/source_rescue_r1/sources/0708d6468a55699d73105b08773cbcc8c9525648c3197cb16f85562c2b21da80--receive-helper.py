# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import base64,hashlib,json,os,stat,subprocess,sys
assert os.getuid()==os.geteuid()==501==os.stat('/dev/console').st_uid and os.getuid()!=0
BASE=Path('/Users/tylerjensen/airk-builds/arbiter-final-control-alias-e306258a-r1');expected={'batch-controller.py': ('191debc8cacb6207a782bf3236afa39fa02ab107dcd2326f47cc0e1da0387058', 17765), 'frozen-config.json': ('890ab242dbeb5157a0f90af6952bfd61e2d923ab1b0b2f4bfd6c930813751a4a', 10562)};dest=BASE/'evidence/platform-system-control-alias-nextest-launch-r1'
for p in [BASE,BASE/'evidence',BASE/'source']:
 s=p.lstat();assert stat.S_ISDIR(s.st_mode) and s.st_uid==501 and p.resolve()==p
assert not os.path.lexists(dest) and not os.path.lexists(BASE/'source/target')
proof=json.loads((BASE/'evidence/source-transfer-proof.json').read_bytes());assert proof['passed'] is True and proof['source_manifest_sha256']=='7d32786855f41e63d99a48d30dddc6d75d9859e9b9f71cc1f1dbe10a8ea94ea0'
raw=sys.stdin.buffer.read(65537);assert len(raw)<=65536;wire=json.loads(raw);assert set(wire)==set(expected);data={k:base64.b64decode(v,validate=True) for k,v in wire.items()}
for k,v in data.items():assert len(v)==expected[k][1] and hashlib.sha256(v).hexdigest()==expected[k][0]
cfg=json.loads(data['frozen-config.json']);assert cfg['commit']=='059a2a893069f89e8ea8aefec7f9bdb8bb6063b6' and cfg['status']=='frozen_declared_overlay_continuation'
assert subprocess.check_output(['git','--no-optional-locks','rev-parse','HEAD'],cwd=BASE/'source').decode().strip()==cfg['commit']
status=subprocess.check_output(['git','--no-optional-locks','status','--porcelain=v1','-z','--untracked-files=all'],cwd=BASE/'source');assert hashlib.sha256(status).hexdigest()==cfg['expected_dirty_status_sha256']
os.umask(0o077);dest.mkdir(mode=0o700)
for k,v in data.items():
 p=dest/k
 with p.open('xb') as f:f.write(v);f.flush();os.fsync(f.fileno())
 s=p.lstat();assert stat.S_ISREG(s.st_mode) and s.st_uid==501 and s.st_nlink==1 and hashlib.sha256(p.read_bytes()).hexdigest()==expected[k][0]
print(json.dumps({'passed':True,'uid':os.getuid(),'console_uid':os.stat('/dev/console').st_uid,'helper_dir':str(dest),'hashes':{k:v[0] for k,v in expected.items()}}))
