# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text())
remote='''set -eu
cd "$HOME/code/airk"
/usr/bin/python3 - <<'PY'
from pathlib import Path
import json
r=Path('artifacts/advanced-split/ort130-arm64/tools')
for line in (r/'cmake-source-build/CMakeCache.txt').read_text().splitlines():
 if line.startswith(('CMAKE_USE_SYSTEM_', 'CMAKE_USE_OPENSSL:', 'CMAKE_BUILD_TYPE:', 'CMAKE_OSX_ARCHITECTURES:')):print(line)
a=json.loads((r/'cmake-source-attempt.json').read_text()); print('RESULT',a['result']); print('PROBE',json.dumps(a.get('probe'))); print('PROGRAM',json.dumps(a.get('program')))
PY
'''
a=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host'],'sh','-s']
r=subprocess.run(a,input=remote.encode(),capture_output=True,timeout=35)
Path('artifacts/advanced-split/receipts/delivery-04-neo-source-cache-effective.log').write_bytes(r.stdout+r.stderr)
print(r.stdout.decode());print(r.stderr.decode());raise SystemExit(r.returncode)

