# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
remote='''set -eu
cd "$HOME/code/airk"
/usr/bin/python3 - <<'PY'
from pathlib import Path
import json,subprocess,os
home=Path.home()
for base in [home/'.rustup/toolchains',home/'.cargo/bin']:
 if base.is_dir():
  for path in sorted(base.iterdir()):
   if base.name=='toolchains' or path.name in ('cargo','rustc','rustup','cargo-nextest'):print('PINNED_TOOL_CANDIDATE',str(path),str(path.resolve()))
PY
'''
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=45)
Path('artifacts/advanced-split/receipts/delivery-04-neo-platform-rust-inventory.log').write_bytes(r.stdout+r.stderr)
print(r.stdout.decode());print(r.stderr.decode());raise SystemExit(r.returncode)
