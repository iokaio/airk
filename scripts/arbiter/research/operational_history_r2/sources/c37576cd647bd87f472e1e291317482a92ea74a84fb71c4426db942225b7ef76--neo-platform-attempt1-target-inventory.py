# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
remote='''set -eu
cd "$HOME/code/airk"
/usr/bin/python3 - <<'PY'
from pathlib import Path
import json,subprocess,os
root=Path.cwd().resolve();target=root/'artifacts/v190/p10-target'
print('TARGET',str(target),'exists',target.exists(),'symlink',target.is_symlink())
for directory in [target/'cargo-timings',target/'nextest/ci']:
 print('EVIDENCE_DIRECTORY',str(directory),directory.is_dir())
 if directory.is_dir():
  for path in directory.iterdir():
   if path.is_file():print('EVIDENCE_FILE',path.name,path.stat().st_size,path.stat().st_mtime)
PY
'''
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=45)
Path('artifacts/advanced-split/receipts/delivery-04-neo-platform-attempt1-target-inventory.log').write_bytes(r.stdout+r.stderr)
print(r.stdout.decode());print(r.stderr.decode());raise SystemExit(r.returncode)
