# SPDX-License-Identifier: Apache-2.0
"""Root-authorized isolated Node prerequisite. SSH credentials never printed."""
import hashlib
import json
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
connection = json.loads((ROOT/'artifacts/v190/neo-connection.local.json').read_bytes())
script = (HERE/'provision-neo-node.sh').read_bytes()
command = ['ssh', '-p', str(connection['port']), '-i', str(connection['identity_path']),
           *connection['ssh_options'], connection['user']+'@'+connection['host'], 'sh', '-s']
started = time.monotonic()
result = subprocess.run(command, input=script, capture_output=True, timeout=300)
(HERE/'neo-node-provision.stdout.log').write_bytes(result.stdout)
(HERE/'neo-node-provision.stderr.log').write_bytes(result.stderr)
receipt = {'schema':'airk.private-neo-prerequisite-command/1','command':'existing SSH setup -> POSIX sh -s',
           'stdin_file':'provision-neo-node.sh','stdin_sha256':hashlib.sha256(script).hexdigest(),
           'ssh_exit':result.returncode,'elapsed_seconds':time.monotonic()-started,'credentials_printed':False}
if result.stdout.strip():
    try: receipt['remote_result'] = json.loads(result.stdout)
    except json.JSONDecodeError: receipt['remote_result_parse']='not_json; inspect retained stdout'
(HERE/'neo-node-provision.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
print(json.dumps(receipt))
raise SystemExit(result.returncode)
