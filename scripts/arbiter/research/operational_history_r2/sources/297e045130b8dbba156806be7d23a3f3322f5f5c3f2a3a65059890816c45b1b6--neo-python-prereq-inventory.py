# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
remote='''set -eu
cd "$HOME/code/airk"
/usr/bin/python3 - <<'PY'
from pathlib import Path
import json,subprocess,os
base=Path('artifacts/advanced-split/ort130-arm64')
for name in ['logs/build.log','logs/aggregate.log','build-attempt.json','build-receipt.json','aggregate-receipt.json']:
 p=base/name
 if p.is_file():
  if p.suffix=='.json':
   v=json.loads(p.read_text()); print('RECEIPT',name,v.get('result'),v.get('error'))
  else:print('TAIL',name,p.read_text(errors='replace')[-1800:])
pid=Path('artifacts/advanced-split/receipts/ort130-mac-source-r6.pid').read_text().strip();r=subprocess.run(['/bin/ps','-p',pid,'-o','pid=,stat=,etime=,comm='],capture_output=True);print('OWNER',r.returncode,r.stdout.decode())
roots=[Path('/opt/homebrew/bin'),Path('/usr/local/bin'),Path('/Library/Frameworks/Python.framework/Versions'),Path.home()/'.pyenv/versions',Path.home()/'.local/bin',Path.home()/'.local/share/uv/python']
for root in roots:
 if root.is_dir():
  print('KNOWN_ROOT',str(root))
  for p in sorted(root.glob('*')):
   if 'python' in p.name.lower() or (root.name in ('Versions','versions','python') and p.is_dir()):print('CANDIDATE',str(p),str(p.resolve()))
for root in [Path('artifacts/tools'),Path('artifacts/advanced-split/tools')]:
 if root.is_dir():
  for p in root.glob('*python*'):print('ARTIFACT_CANDIDATE',str(p))
PY
'''
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=45)
Path('artifacts/advanced-split/receipts/delivery-04-neo-python-prereq-inventory.log').write_bytes(r.stdout+r.stderr)
print(r.stdout.decode());print(r.stderr.decode());raise SystemExit(r.returncode)
