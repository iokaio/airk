# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
connection=json.loads(Path("artifacts/v190/neo-connection.local.json").read_text())
command=["ssh",*connection["ssh_options"],"-i",connection["identity_path"],"-p",str(connection["port"]),connection["user"]+"@"+connection["host"],"sh","-s"]
remote="""set -eu
cd "$HOME/code/airk"
/usr/bin/python3 - <<'PY'
import importlib.util,json
from pathlib import Path
spec=importlib.util.spec_from_file_location('core',Path('scripts/provision-static-onnx-runtime.py'))
core=importlib.util.module_from_spec(spec);spec.loader.exec_module(core)
root=Path('artifacts/advanced-split/ort130-arm64/tools').resolve()
program=root/'cmake/CMake.app/Contents/bin/cmake'
log=core.probe_log(root,'cmake-diagnosis')
result=core.run_owned([program,'--version'],root,log,15)
print(json.dumps(result,indent=2))
print('ACTUAL_LOG',repr(log.read_bytes()))
PY
"""
r=subprocess.run(command,input=remote.encode(),capture_output=True,timeout=45)
Path('artifacts/advanced-split/receipts/delivery-04-neo-owned-cmake-diagnosis.log').write_bytes(r.stdout+r.stderr)
print(r.stdout.decode(errors='replace'));print(r.stderr.decode(errors='replace'));raise SystemExit(r.returncode)
