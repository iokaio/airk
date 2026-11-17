# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
remote='''set -eu
cd "$HOME/code/airk"
/usr/bin/python3 - <<'PY'
from pathlib import Path
import json,subprocess,os
root=Path.cwd().resolve()
for name in ['artifacts/v190/p10-target/nextest/ci/junit.xml','target/nextest/ci/junit.xml','junit.xml','.config/junit.xml']:
 path=root/name
 if path.is_file():
  data=path.read_bytes();assert len(data)<2*1024*1024
  out=root/'artifacts/advanced-split/receipts/mac-platform-scoped'/'mac-platform-tests-attempt4-observed-junit.xml'
  if not out.exists():out.write_bytes(data)
  print('JUNIT_RETAINED',name,len(data))
 else:print('JUNIT_ABSENT',name)
PY
'''
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=45)
Path('artifacts/advanced-split/receipts/mac-platform-attempt4-junit-location.log').write_bytes(r.stdout+r.stderr)
print(r.stdout.decode());print(r.stderr.decode());raise SystemExit(r.returncode)
