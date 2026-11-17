# SPDX-License-Identifier: Apache-2.0
#!/bin/sh
# Read-only observation: no Cargo, cleanup, source writes or source apply.
set -eu
cd "$HOME/code/airk"
"$PWD/artifacts/tools/neo-python-3.13.18/bin/python3.13" -I -B - <<'PY'
import datetime,hashlib,json,os,subprocess,sys
from pathlib import Path
assert sys.version_info[:3]==(3,13,18)
root=Path.cwd().resolve();assert os.path.samefile(root,Path.home()/'code/airk')
manifest=root/'artifacts/advanced-split/receipts/mac-cli-await-source-r6.json';raw=manifest.read_bytes();assert hashlib.sha256(raw).hexdigest()=='908e962c94f48f48f6f2301fc45b40d1d7080b74f29ffd6934cdedf5c59dde51'
v=json.loads(raw);target=root/'artifacts/v190/p10-target'
rows=subprocess.check_output(['/bin/ps','-axo','pid=,ppid=,comm='],text=True).splitlines();active=[]
for line in rows:
 fields=line.split(None,2)
 if len(fields)==3 and Path(fields[2].strip()).name in ('cargo','cargo-nextest','rustc','clippy-driver','rustfmt'):active.append({'pid':int(fields[0]),'ppid':int(fields[1]),'image':Path(fields[2].strip()).name})
state={'observed_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'physical_root':str(root),'lexical_shell_pwd':os.environ.get('PWD'),'approved_workspace_same_object':True,'applied_source_seal':v['source_set_sha256'],'manifest_sha256':hashlib.sha256(raw).hexdigest(),'retired_build_still_absent':not (root/'crates/airk-cli/build.rs').exists(),'active_compilers':active,'target':str(target),'target_exists':target.exists(),'target_symlink':target.is_symlink(),'r2_receipts_preserved':(root/'artifacts/advanced-split/receipts/mac-cli-await-r2').is_dir(),'source_apply_recovery_preserved':(root/'artifacts/advanced-split/receipts/mac-cli-await-transfer-r2/recovery.json').is_file()}
if target.exists() and not target.is_symlink():
 info=target.stat();state.update(target_device=info.st_dev,target_inode=info.st_ino,target_entries=sorted(p.name for p in target.iterdir()))
print(json.dumps(state,indent=2))
PY
