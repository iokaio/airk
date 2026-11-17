# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
connection=json.loads(Path("artifacts/v190/neo-connection.local.json").read_text())
command=["ssh",*connection["ssh_options"],"-i",connection["identity_path"],"-p",str(connection["port"]),connection["user"]+"@"+connection["host"],"sh","-s"]
remote="""set -eu
cd "$HOME/code/airk"
PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3 - <<'PY'
from pathlib import Path
import subprocess,json
root=Path('artifacts/advanced-split/ort130-arm64/tools').resolve();binary=root/'cmake/CMake.app/Contents/bin/cmake'
for argv in [['/usr/bin/codesign','--verify','--strict','--verbose=2',str(binary)],['/usr/bin/codesign','-dvv',str(binary)],['/usr/bin/xattr','-l',str(binary)],['/usr/sbin/spctl','--assess','--type','execute','--verbose=2',str(binary)],['/usr/bin/log','show','--last','3m','--style','compact','--predicate','process == "syspolicyd" AND eventMessage CONTAINS[c] "cmake"']]:
 try:
  r=subprocess.run(argv,capture_output=True,timeout=20);print('READONLY',argv[0],r.returncode,repr(r.stdout[:20000]),repr(r.stderr[:8000]),flush=True)
 except subprocess.TimeoutExpired:print('READONLY_TIMEOUT',argv[0],flush=True)
for path in sorted((root/'logs').glob('probe-cmake-repaired-*/result.json')):print('RESULT',path.read_text())
PY
"""
r=subprocess.run(command,input=remote.encode(),capture_output=True,timeout=100)
Path('artifacts/advanced-split/receipts/delivery-04-neo-cmake-policy-diagnosis.log').write_bytes(r.stdout+r.stderr)
print(r.stdout.decode(errors='replace'));print(r.stderr.decode(errors='replace'));raise SystemExit(r.returncode)
