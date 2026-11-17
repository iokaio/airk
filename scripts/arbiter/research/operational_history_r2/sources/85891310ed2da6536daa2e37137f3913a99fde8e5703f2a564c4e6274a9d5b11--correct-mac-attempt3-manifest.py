# SPDX-License-Identifier: Apache-2.0
import hashlib,json,subprocess
from pathlib import Path
p=Path('artifacts/advanced-split/receipts/mac-platform-correction-r1-source.json');v=json.loads(p.read_text());content=(json.dumps(v,indent=2,sort_keys=True)+'\n').encode();p.write_bytes(content);sha=hashlib.sha256(content).hexdigest()
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
remote_path='artifacts/advanced-split/receipts/mac-platform-correction-r1-source.json'
command="'cd \"$HOME/code/airk\" && test ! -e artifacts/advanced-split/receipts/mac-platform-correction-r1/source-manifest-before-serialization-fix.json && cp "+remote_path+" artifacts/advanced-split/receipts/mac-platform-correction-r1/source-manifest-before-serialization-fix.json && cat > "+remote_path+"'"
r=subprocess.run(ssh+['sh','-c',command],input=content,capture_output=True,timeout=30)
if r.returncode:raise SystemExit(r.stderr.decode())
p=Path('artifacts/advanced-split/mac-platform-scoped-attempt3.sh');s=p.read_text().replace('06150d623823473ce83f15c35196db74de2dadf2963a3feccc6d5c2ecb270cfd',sha);p.write_text(s,encoding='utf-8',newline='\n')
s=Path('artifacts/advanced-split/check-mac-attempt2-source.py').read_text().replace('mac-platform-scoped-attempt2.sh','mac-platform-scoped-attempt3.sh').replace('mac-platform-ec3c3238ca7a52ae','mac-platform-23b2bdea47b350fe').replace('mac-format-r1/attempt2-source-check.log','mac-platform-correction-r1-preflight.log');Path('artifacts/advanced-split/check-mac-attempt3-source.py').write_text(s)
print('EXACT_MANIFEST_SHA256',sha)
