# SPDX-License-Identifier: Apache-2.0
"""Root invokes only after explicit single-Cargo-owner baton handoff."""
import json,subprocess,sys
sys.stdout.reconfigure(encoding="utf-8")
sys.stderr.reconfigure(encoding="utf-8")
from pathlib import Path
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
script=Path('artifacts/advanced-split/mac-platform-scoped-attempt4.sh').read_text()
# This explicit baton belongs to root invocation, never launched by the preparer.
script='AIRK_CARGO_OWNER_BATON=mac-platform-be56d153c8e43fdd\nexport AIRK_CARGO_OWNER_BATON\n'+script
r=subprocess.run(ssh+['sh','-s'],input=script.encode(),capture_output=True)
Path('artifacts/advanced-split/receipts/mac-platform-scoped-attempt4-transport.log').write_bytes(r.stdout+r.stderr)
print(r.stdout.decode());print(r.stderr.decode());raise SystemExit(r.returncode)
