# SPDX-License-Identifier: Apache-2.0
import hashlib,io,json,subprocess,tarfile
from pathlib import Path
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
remote='''set -eu
cd "$HOME/code/airk"
PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3 -c "print('READONLY_VERIFY')"
PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3 scripts/provision-static-onnx-runtime.py --target aarch64-apple-darwin --destination artifacts/advanced-split/ort130-arm64 --verify --environment-file artifacts/advanced-split/ort130-arm64-env.json
'''
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=390)
Path('artifacts/advanced-split/receipts/delivery-04-neo-aggregate-readonly-verify.log').write_bytes(r.stdout+r.stderr)
Path('artifacts/advanced-split/receipts/delivery-04-neo-aggregate-readonly-verify.json').write_text(json.dumps({'command':remote,'exit_code':r.returncode,'stdout':r.stdout.decode(),'stderr':r.stderr.decode(),'scope':'genuine existing completed ORT archive aggregation only; no CMake/ORT rebuild, no Cargo/model transfer/trust changes'},indent=2),encoding='utf-8')
print(r.stdout.decode()[-5000:]);print(r.stderr.decode()[-2500:]);raise SystemExit(r.returncode)
