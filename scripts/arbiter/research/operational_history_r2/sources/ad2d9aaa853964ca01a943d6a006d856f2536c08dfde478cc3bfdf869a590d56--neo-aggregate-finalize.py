# SPDX-License-Identifier: Apache-2.0
import hashlib,io,json,subprocess,tarfile
from pathlib import Path
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
names=['scripts/static-onnx-aggregate.py','scripts/tests/test_static_ort_aggregate.py'];data=io.BytesIO();seals={}
with tarfile.open(fileobj=data,mode='w:gz') as t:
 for name in names:
  content=Path(name).read_bytes();seals[name]=hashlib.sha256(content).hexdigest();r=tarfile.TarInfo(name);r.size=len(content);r.mode=0o644;t.addfile(r,io.BytesIO(content))
r=subprocess.run(ssh+['sh','-c',"'cd \"$HOME/code/airk\" && tar -xzf -'"],input=data.getvalue(),capture_output=True,timeout=35)
if r.returncode:raise RuntimeError(r.stderr.decode())
remote='''set -eu
cd "$HOME/code/airk"
PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3 -m unittest scripts.tests.test_static_ort_aggregate
PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3 scripts/provision-static-onnx-runtime.py --target aarch64-apple-darwin --destination artifacts/advanced-split/ort130-arm64 --aggregate-static --environment-file artifacts/advanced-split/ort130-arm64-env.json
'''
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=390)
Path('artifacts/advanced-split/receipts/delivery-04-neo-aggregate-finalize.log').write_bytes(r.stdout+r.stderr)
Path('artifacts/advanced-split/receipts/delivery-04-neo-aggregate-finalize.json').write_text(json.dumps({'source_sha256':seals,'command':remote,'exit_code':r.returncode,'stdout':r.stdout.decode(),'stderr':r.stderr.decode(),'scope':'genuine existing completed ORT archive aggregation only; no CMake/ORT rebuild, no Cargo/model transfer/trust changes'},indent=2),encoding='utf-8')
print(r.stdout.decode()[-5000:]);print(r.stderr.decode()[-2500:]);raise SystemExit(r.returncode)
