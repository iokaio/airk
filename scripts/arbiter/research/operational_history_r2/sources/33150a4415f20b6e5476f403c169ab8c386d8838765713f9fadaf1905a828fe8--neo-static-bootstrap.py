# SPDX-License-Identifier: Apache-2.0
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tarfile

root = Path.cwd()
connection = json.loads(Path('artifacts/v190/neo-connection.local.json').read_text())
ssh = ['ssh', *connection['ssh_options'], '-i', connection['identity_path'],
       '-p', str(connection['port']), connection['user'] + '@' + connection['host']]
names = ['provision-static-onnx-runtime.py', 'provision-static-onnx-tools.py',
         'static-onnx-runtime-lock.json', 'static-onnx-tools-lock.json',
         'static-onnx-link-closure.py', 'static-onnx-aggregate.py', 'darwin-waitid.py']
archive = io.BytesIO()
seals = {}
with tarfile.open(fileobj=archive, mode='w:gz') as tar:
    for name in names:
        path = root / 'scripts' / name
        data = path.read_bytes()
        seals['scripts/' + name] = hashlib.sha256(data).hexdigest()
        entry = tarfile.TarInfo('scripts/' + name)
        entry.size = len(data)
        entry.mode = 0o644
        tar.addfile(entry, io.BytesIO(data))
result = subprocess.run(ssh + ['sh', '-c', "'cd \"$HOME/code/airk\" && tar -xzf -'"],
                        input=archive.getvalue(), capture_output=True, timeout=45)
if result.returncode:
    print(result.stderr.decode())
    raise SystemExit(result.returncode)
script = '''set -eu
cd "$HOME/code/airk"
py=''
for candidate in /opt/homebrew/bin/python3 /usr/local/bin/python3 "$HOME/.local/bin/python3" /usr/bin/python3; do
  if test -x "$candidate" && "$candidate" -c 'import sys; assert sys.version_info >= (3,9)' >/dev/null 2>&1; then py=$candidate; break; fi
done
if test -z "$py"; then echo 'Python >=3.9 not found in known host tool locations'; exit 27; fi
mkdir -p artifacts/advanced-split/receipts
if test -e artifacts/advanced-split/receipts/ort130-mac-start-r4.pid; then echo 'Existing owned start retained; refusing duplicate launch'; exit 28; fi
nohup "$py" scripts/provision-static-onnx-runtime.py --target aarch64-apple-darwin --destination artifacts/advanced-split/ort130-arm64 --download --download-tools --build --parallel 2 --environment-file artifacts/advanced-split/ort130-arm64-env.json > artifacts/advanced-split/receipts/ort130-mac-build-r4.log 2>&1 < /dev/null &
printf '%s\n' "$!" > artifacts/advanced-split/receipts/ort130-mac-start-r4.pid
printf 'Started owned static ORT build pid=%s python=%s\n' "$!" "$py"
'''
result = subprocess.run(ssh + ['sh', '-s'], input=script.encode(), capture_output=True, timeout=45)
receipt = {'source_sha256': seals, 'exit_code': result.returncode,
           'stdout': result.stdout.decode(), 'stderr': result.stderr.decode(),
           'scope': 'Mac arm64 static ORT prerequisite, no Cargo or native qualification'}
Path('artifacts/advanced-split/receipts/delivery-03-neo-static-start-r4.json').write_text(json.dumps(receipt, indent=2))
print(receipt['stdout'])
print(receipt['stderr'])
raise SystemExit(result.returncode)
