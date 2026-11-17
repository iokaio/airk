# SPDX-License-Identifier: Apache-2.0
import json,subprocess
from pathlib import Path
connection=json.loads(Path("artifacts/v190/neo-connection.local.json").read_text())
command=["ssh",*connection["ssh_options"],"-i",connection["identity_path"],"-p",str(connection["port"]),connection["user"]+"@"+connection["host"],"sh","-s"]
remote="""set -eu
cd "$HOME/code/airk"
PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3 - <<'PY'
from pathlib import Path
import json,subprocess
base=Path('artifacts/advanced-split/ort130-arm64/tools');receipt=base/'cmake-source-attempt.json'
print('SOURCE_ATTEMPT',receipt.is_file())
if receipt.is_file():
 value=json.loads(receipt.read_text());print('SOURCE_STATE',value['result']);print('FINISHED_STAGES',[row['stage'] for row in value['commands']])
print('BOOTSTRAP_OBJECTS',len(list((base/'cmake-source-build/Bootstrap.cmk').glob('*.o'))))
for name in ['artifacts/advanced-split/receipts/ort130-mac-source-r6.log','artifacts/advanced-split/ort130-arm64/tools/logs/ninja-build.log','artifacts/advanced-split/ort130-arm64/logs/configure.log','artifacts/advanced-split/ort130-arm64/logs/build.log']:
 path=Path(name)
 if path.is_file():print('TAIL',name,path.read_text(errors='replace')[-2500:])
for name in ['artifacts/advanced-split/ort130-arm64/tools/receipt.json','artifacts/advanced-split/ort130-arm64/build-attempt.json']:
 path=Path(name)
 if path.is_file():
  value=json.loads(path.read_text());print('RECEIPT',name,value.get('result'),value.get('error'),[r.get('stage') for r in value.get('commands',[])])
pid=Path('artifacts/advanced-split/receipts/ort130-mac-source-r6.pid').read_text().strip();r=subprocess.run(['/bin/ps','-p',pid,'-o','pid=,stat=,etime=,comm='],capture_output=True);print('OWNER',r.returncode,r.stdout.decode())
PY
"""
r=subprocess.run(command,input=remote.encode(),capture_output=True,timeout=45)
Path('artifacts/advanced-split/receipts/delivery-04-neo-static-source-ort-progress-2.log').write_bytes(r.stdout+r.stderr)
print(r.stdout.decode(errors='replace'));print(r.stderr.decode(errors='replace'));raise SystemExit(r.returncode)


