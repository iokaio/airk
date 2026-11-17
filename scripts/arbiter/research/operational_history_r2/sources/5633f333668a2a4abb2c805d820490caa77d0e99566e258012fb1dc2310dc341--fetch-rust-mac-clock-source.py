# SPDX-License-Identifier: Apache-2.0
import hashlib,json,subprocess
from pathlib import Path
path='library/std/src/sys/time/unix.rs';commit='88d9e12ae178fab0fb5cc050a94da85685d449ea'
r=subprocess.run(['gh','api','repos/rust-lang/rust/contents/'+path+'?ref='+commit,'-H','Accept: application/vnd.github.raw+json'],capture_output=True,check=True,timeout=30);data=r.stdout
assert hashlib.sha1(b'blob '+str(len(data)).encode()+bytes([0])+data).hexdigest()=='944cb552cad9ef84348dc6ea35287a60bd2453c1'
p=Path('artifacts/advanced-split/receipts/mac-platform-scoped-attempt4/rust-1.98-unix-time.rs');p.write_bytes(data);print('SOURCE_SHA256',hashlib.sha256(data).hexdigest())
lines=data.decode().splitlines()
for index,line in enumerate(lines):
 if 'CLOCK_UPTIME_RAW' in line:
  print('\n'.join(str(i+1)+': '+lines[i] for i in range(max(0,index-9),min(len(lines),index+11))))
