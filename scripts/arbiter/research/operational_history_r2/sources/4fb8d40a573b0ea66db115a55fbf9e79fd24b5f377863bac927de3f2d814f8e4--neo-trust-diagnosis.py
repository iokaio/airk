# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
connection=json.loads(Path("artifacts/v190/neo-connection.local.json").read_text())
command=["ssh",*connection["ssh_options"],"-i",connection["identity_path"],"-p",str(connection["port"]),connection["user"]+"@"+connection["host"],"sh","-s"]
remote="""set -eu
cd "$HOME/code/airk"
PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3 - <<'PY'
import subprocess
queries=[['/usr/bin/log','show','--last','4m','--style','compact','--info','--debug','--predicate','process == "trustd" AND (eventMessage CONTAINS[c] "ocsp" OR eventMessage CONTAINS[c] "revocation" OR eventMessage CONTAINS[c] "timeout" OR eventMessage CONTAINS[c] "failure")'],['/usr/bin/log','show','--last','4m','--style','compact','--info','--debug','--predicate','process == "amfid" AND (eventMessage CONTAINS[c] "error" OR eventMessage CONTAINS[c] "fail" OR eventMessage CONTAINS[c] "deny")'],['/bin/ps','-A','-o','pid=,stat=,etime=,comm=']]
for argv in queries:
 try:
  r=subprocess.run(argv,capture_output=True,timeout=15);text=r.stdout.decode(errors='replace')
  if argv[0]=='/bin/ps':text=chr(10).join(line for line in text.splitlines() if any(name in line for name in ['amfid','trustd','syspolicyd']))
  print('DIAG',argv[0],r.returncode,text[-22000:],r.stderr.decode(errors='replace')[:1000],flush=True)
 except subprocess.TimeoutExpired:print('READONLY_DIAG_TIMEOUT',argv[0])
PY
"""
r=subprocess.run(command,input=remote.encode(),capture_output=True,timeout=60)
Path('artifacts/advanced-split/receipts/delivery-04-neo-trust-diagnosis.log').write_bytes(r.stdout+r.stderr)
print(r.stdout.decode(errors='replace'));print(r.stderr.decode(errors='replace'));raise SystemExit(r.returncode)
