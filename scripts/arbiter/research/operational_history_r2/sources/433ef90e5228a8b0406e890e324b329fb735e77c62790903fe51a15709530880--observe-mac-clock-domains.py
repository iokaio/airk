# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
remote='''set -eu
cd "$HOME/code/airk"
/usr/bin/python3 - <<'PY'
import ctypes,hashlib,json,subprocess
from pathlib import Path
root=Path.home()/'.rustup/toolchains/1.98.0-aarch64-apple-darwin';result={}
r=subprocess.run([str(root/'bin/rustc'),'--version','--verbose'],capture_output=True,text=True,timeout=15);result['rustc']={'exit_code':r.returncode,'stdout':r.stdout,'stderr':r.stderr}
result['installed_rust_sources']={}
for relative in ['library/std/src/sys/time/unix.rs','library/std/src/sys/time/apple.rs','library/std/src/sys/pal/unix/time.rs']:
 path=root/'lib/rustlib/src/rust'/relative
 if path.is_file():
  source=path.read_text();result['installed_rust_sources'][str(path)]={'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'text':source}
header=Path('/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk/usr/include/time.h')
if header.is_file():result['sdk_clock_declarations']=[line for line in header.read_text().splitlines() if any(s in line for s in ['CLOCK_MONOTONIC','CLOCK_UPTIME_RAW','clock_getres','clock_gettime'])]
class Timespec(ctypes.Structure):_fields_=[('seconds',ctypes.c_long),('nanoseconds',ctypes.c_long)]
lib=ctypes.CDLL('/usr/lib/libSystem.B.dylib',use_errno=True)
for name in ['clock_gettime','clock_getres']:
 fn=getattr(lib,name);fn.argtypes=[ctypes.c_int,ctypes.POINTER(Timespec)];fn.restype=ctypes.c_int
result['clocks']={}
for number,name in [(6,'CLOCK_MONOTONIC'),(8,'CLOCK_UPTIME_RAW')]:
 value=Timespec();assert lib.clock_getres(number,ctypes.byref(value))==0;result['clocks'][name]={'id':number,'resolution_ns':value.seconds*1000000000+value.nanoseconds}
samples=[]
for _ in range(5):
 row={}
 for number,name in [(6,'monotonic_ns'),(8,'uptime_raw_ns')]:
  value=Timespec();assert lib.clock_gettime(number,ctypes.byref(value))==0;row[name]=value.seconds*1000000000+value.nanoseconds
 row['sequential_sample_difference_ns']=row['monotonic_ns']-row['uptime_raw_ns'];samples.append(row)
 sum(range(10000))
result['samples']=samples
print(json.dumps(result))
PY
'''
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=35)
if r.returncode:raise SystemExit(r.stderr.decode())
v=json.loads(r.stdout);out=Path('artifacts/advanced-split/receipts/mac-platform-scoped-attempt4/mac-clock-domain-observation.json');out.write_text(json.dumps(v,indent=2)+'\n',encoding='utf-8');print(json.dumps({k:value for k,value in v.items() if k!='installed_rust_sources'},indent=2));print('INSTALLED_RUST_SOURCE_FILES',list(v['installed_rust_sources']))
