# SPDX-License-Identifier: Apache-2.0
"""Read-only names of possible previously downloaded pinned Node assets."""
import json
from pathlib import Path
import subprocess
import time
ROOT = Path(__file__).resolve().parents[3]
connection = json.loads((ROOT / 'artifacts/v190/neo-connection.local.json').read_bytes())
remote = r'''import json,pwd,os
from pathlib import Path
home=Path(pwd.getpwuid(os.geteuid()).pw_dir)
parents=[home/'Downloads',home/'code/airk/artifacts/tools',home/'code/airk/artifacts/advanced-split',home/'Library/Caches/Homebrew/downloads',Path('/opt/homebrew/Cellar')]
rows=[]
for parent in parents:
 if not parent.is_dir():continue
 for child in parent.iterdir():
  name=child.name.casefold()
  if 'node' in name or 'shasums256' in name:
   rows.append({'path':str(child),'directory':child.is_dir(),'bytes':child.stat().st_size if child.is_file() else None})
print(json.dumps({'schema':'airk.private-neo-node-asset-names/1','read_only':True,'candidates':rows}))
'''
started = time.monotonic()
result = subprocess.run(['ssh', '-p', str(connection['port']), '-i', str(connection['identity_path']),
    *connection['ssh_options'], connection['user'] + '@' + connection['host'], '/usr/bin/python3', '-'],
    input=remote, capture_output=True, text=True, timeout=45)
receipt = {'ssh_exit': result.returncode, 'elapsed_seconds': time.monotonic() - started, 'credentials_printed': False}
if result.returncode == 0:
    receipt['observation'] = json.loads(result.stdout)
path = Path(__file__).with_name('neo-node-assets-readonly.json')
assert not path.exists()
path.write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')
print(json.dumps(receipt))
raise SystemExit(result.returncode)
