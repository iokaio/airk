# SPDX-License-Identifier: Apache-2.0
"""Root invokes only after explicit single-Cargo-owner baton handoff."""
import json,subprocess
from pathlib import Path
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
script=Path('artifacts/advanced-split/mac-platform-scoped-batch.sh').read_text()
# This explicit baton belongs to root invocation, never launched by the preparer.
script='AIRK_CARGO_OWNER_BATON=mac-platform-7d913a63b20692b4\nexport AIRK_CARGO_OWNER_BATON\n'+script
r=subprocess.run(ssh+['sh','-s'],input=script.encode(),capture_output=True)
Path('artifacts/advanced-split/receipts/mac-platform-scoped-batch.log').write_bytes(r.stdout+r.stderr)
print(r.stdout.decode());print(r.stderr.decode());raise SystemExit(r.returncode)
