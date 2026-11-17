# SPDX-License-Identifier: Apache-2.0
import json
import subprocess
from pathlib import Path
c = json.loads(Path('artifacts/v190/neo-connection.local.json').read_text())
argv = ['ssh', *c['ssh_options'], '-i', c['identity_path'], '-p', str(c['port']), c['user']+'@'+c['host'], 'sh', '-s']
script = b'''set -eu
cd "$HOME/code/airk"
/usr/bin/python3 --version
cat artifacts/advanced-split/ort130-arm64/tools/logs/probe-cmake-bxjw7m6g/output.log
find artifacts/advanced-split/ort130-arm64/tools/logs/probe-cmake-bxjw7m6g -type f -maxdepth 1
'''
r = subprocess.run(argv, input=script, capture_output=True, timeout=45)
Path('artifacts/advanced-split/receipts/delivery-04-neo-probe-diagnostics.log').write_bytes(r.stdout+r.stderr)
print(r.stdout.decode()); print(r.stderr.decode()); raise SystemExit(r.returncode)
