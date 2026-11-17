# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
remote='''set -eu
cd "$HOME/code/airk"
PYTHONDONTWRITEBYTECODE=1 artifacts/tools/neo-python-3.13.18/bin/python3.13 -m unittest scripts.tests.test_receipt_runner
'''
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=45);Path('artifacts/advanced-split/receipts/mac-platform-correction-r3-runner-tests.log').write_bytes(r.stdout+r.stderr);print(r.stdout.decode());print(r.stderr.decode());raise SystemExit(r.returncode)
