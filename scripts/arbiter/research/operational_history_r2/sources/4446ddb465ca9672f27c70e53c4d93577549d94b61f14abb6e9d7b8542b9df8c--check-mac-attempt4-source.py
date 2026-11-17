# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
s=Path('artifacts/advanced-split/mac-platform-scoped-attempt4.sh').read_text();s=s[:s.index('# Formatting and selected Clippy')];s='AIRK_CARGO_OWNER_BATON=mac-platform-be56d153c8e43fdd\nexport AIRK_CARGO_OWNER_BATON\n'+s
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
r=subprocess.run(ssh+['sh','-s'],input=s.encode(),capture_output=True,timeout=35);Path('artifacts/advanced-split/receipts/mac-platform-correction-r2-preflight.log').write_bytes(r.stdout+r.stderr);print(r.stdout.decode());print(r.stderr.decode());raise SystemExit(r.returncode)
