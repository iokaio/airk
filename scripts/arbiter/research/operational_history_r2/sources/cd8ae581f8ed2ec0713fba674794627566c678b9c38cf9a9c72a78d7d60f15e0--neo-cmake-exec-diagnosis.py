# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
connection=json.loads(Path("artifacts/v190/neo-connection.local.json").read_text())
command=["ssh",*connection["ssh_options"],"-i",connection["identity_path"],"-p",str(connection["port"]),connection["user"]+"@"+connection["host"],"sh","-s"]
remote="""set -eu
cd "$HOME/code/airk"
PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3 - <<'PY'
from pathlib import Path
import importlib.util,json,subprocess
spec=importlib.util.spec_from_file_location('core',Path('scripts/provision-static-onnx-runtime.py'));core=importlib.util.module_from_spec(spec);spec.loader.exec_module(core)
root=Path('artifacts/advanced-split/ort130-arm64/tools').resolve();binary=root/'cmake/CMake.app/Contents/bin/cmake'
r=subprocess.run(['/usr/sbin/spctl','--assess','--type','execute','--verbose=2',str(root/'cmake/CMake.app')],capture_output=True,timeout=20);print('APP_ASSESSMENT',r.returncode,repr(r.stdout),repr(r.stderr),flush=True)
for label,argv in [('system-exec',['/bin/sh','-c','exec "$@"','owned-probe',str(binary),'--version']),('explicit-arm64',['/usr/bin/arch','-arm64',str(binary),'--version'])]:
 log=core.probe_log(root,label);result=core.run_owned(argv,root,log,15);core.write_json(log.parent/'result.json',result);print(label,json.dumps(result),repr(log.read_bytes()),flush=True)
 if core.successful(result):break
PY
"""
r=subprocess.run(command,input=remote.encode(),capture_output=True,timeout=80)
Path('artifacts/advanced-split/receipts/delivery-04-neo-cmake-exec-diagnosis.log').write_bytes(r.stdout+r.stderr)
print(r.stdout.decode(errors='replace'));print(r.stderr.decode(errors='replace'));raise SystemExit(r.returncode)
