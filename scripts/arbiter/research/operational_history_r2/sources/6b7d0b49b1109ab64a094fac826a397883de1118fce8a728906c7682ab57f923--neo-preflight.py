# SPDX-License-Identifier: Apache-2.0
import json
import subprocess
from pathlib import Path

connection = json.loads(Path('artifacts/v190/neo-connection.local.json').read_text())
command = ['ssh', *connection['ssh_options'], '-i', connection['identity_path'],
           '-p', str(connection['port']), connection['user'] + '@' + connection['host'], 'sh', '-s']
script = '''set -eu
uname -sm
getconf _NPROCESSORS_ONLN
df -Pk "$HOME/code/airk"
cd "$HOME/code/airk"
git status --short
command -v python3
command -v cmake || true
command -v ninja || true
find artifacts -name '*ort*environment*.json' -o -name 'build-receipt.json' 2>/dev/null | head -20
tail -8 artifacts/advanced-split/receipts/ort130-mac-build-r4.log 2>/dev/null || true
if test -f artifacts/advanced-split/receipts/ort130-mac-start-r4.pid; then
    read build_pid < artifacts/advanced-split/receipts/ort130-mac-start-r4.pid
    ps -p "$build_pid" -o pid=,comm= || true
fi
'''
result = subprocess.run(command, input=script.encode(), capture_output=True, timeout=45)
Path('artifacts/advanced-split/receipts/delivery-04-neo-preflight.log').write_bytes(result.stdout + result.stderr)
print(result.stdout.decode())
print('SSH exit:', result.returncode)
raise SystemExit(result.returncode)
