# SPDX-License-Identifier: Apache-2.0
#!/bin/sh
# Read-only remote observation. No transfer/Cargo/source mutation/cleanup.
set -eu
cd "$HOME/code/airk"
"$PWD/artifacts/tools/neo-python-3.13.18/bin/python3.13" -I -B - <<'PY'
import datetime, hashlib, json, os, shutil, subprocess, sys
from pathlib import Path
root=Path.cwd().resolve()
assert sys.version_info[:3]==(3,13,18) and os.path.samefile(root,Path.home()/'code/airk')
manifest=root/'artifacts/advanced-split/receipts/mac-cli-await-source-r12.json'
raw=manifest.read_bytes()
assert hashlib.sha256(raw).hexdigest()=='0326f4363d9eb939ef6ef0ba730cf3f044447ffbf787df7fb3da7cce94c664a4'
previous=json.loads(raw)
actual={}
for name in previous['files']:
    p=root/name
    assert not p.is_symlink()
    actual[name]=hashlib.sha256(p.read_bytes()).hexdigest() if p.is_file() else None
rows=subprocess.check_output(['/bin/ps','-axo','pid=,ppid=,comm='],text=True).splitlines()
active=[]
for line in rows:
    fields=line.split(None,2)
    if len(fields)==3 and Path(fields[2].strip()).name in ('cargo','cargo-nextest','rustc','clippy-driver','rustfmt','cc1','clang','clang++','ninja','cmake'):
        active.append({'pid':int(fields[0]),'ppid':int(fields[1]),'image':Path(fields[2].strip()).name})
env=dict(os.environ)
env['PATH']=str(root/'artifacts/tools/neo-nextest-0.9.148')+':'+str(Path.home()/'.cargo/bin')+':'+env.get('PATH','')
tools=[]
for command in [['cargo','--version'],['rustup','show','active-toolchain'],['cargo','nextest','--version']]:
    result=subprocess.run(command,env=env,capture_output=True,text=True,timeout=10)
    tools.append({'command':command,'exit_code':result.returncode,'stdout':result.stdout.strip(),'stderr_sha256':hashlib.sha256(result.stderr.encode()).hexdigest()})
disk=shutil.disk_usage(root)
target=root/'artifacts/v190/p10-target'
print(json.dumps({'observed_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'physical_workspace':str(root),
 'approved_workspace_same_object':True,'head':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
 'branch':subprocess.check_output(['git','branch','--show-current'],text=True).strip(),
 'public_git_status':subprocess.check_output(['git','status','--porcelain','--untracked-files=normal'],text=True).splitlines(),
 'manifest_sha256':hashlib.sha256(raw).hexdigest(),'previous_source_set_sha256':previous['source_set_sha256'],
 'previous_files':previous['files'],'actual_known_source_sha256':actual,
 'retired_auto_discovery':{name:(root/name).exists() for name in previous['retired_auto_discovery']},
 'active_compilers':active,'disk_total_bytes':disk.total,'disk_free_bytes':disk.free,
 'target_exists':target.exists(),'target_symlink':target.is_symlink(),
 'tools':tools,'python_version':list(sys.version_info[:3]),'no_writes_or_native_builds':True}))
PY
