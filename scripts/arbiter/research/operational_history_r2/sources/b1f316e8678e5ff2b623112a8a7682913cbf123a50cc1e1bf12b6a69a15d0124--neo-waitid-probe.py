# SPDX-License-Identifier: Apache-2.0
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tarfile

connection = json.loads(Path('artifacts/v190/neo-connection.local.json').read_text())
ssh = ['ssh', *connection['ssh_options'], '-i', connection['identity_path'],
       '-p', str(connection['port']), connection['user'] + '@' + connection['host']]
archive = io.BytesIO()
seals = {}
with tarfile.open(fileobj=archive, mode='w:gz') as tar:
    for name in ['provision-static-onnx-runtime.py', 'darwin-waitid.py']:
        data = (Path('scripts') / name).read_bytes()
        seals[name] = hashlib.sha256(data).hexdigest()
        entry = tarfile.TarInfo('scripts/' + name)
        entry.size = len(data)
        entry.mode = 0o644
        tar.addfile(entry, io.BytesIO(data))
transferred = subprocess.run(ssh + ['sh', '-c', "'cd \"$HOME/code/airk\" && tar -xzf -'"], input=archive.getvalue(), capture_output=True, timeout=45)
assert transferred.returncode == 0, transferred.stderr.decode()
code = '''import importlib.util, json, os, pathlib, sys
root = pathlib.Path.home() / 'code/airk'
spec = importlib.util.spec_from_file_location('ort_owner', root/'scripts/provision-static-onnx-runtime.py')
core = importlib.util.module_from_spec(spec); spec.loader.exec_module(core)
out = root/'artifacts/advanced-split/darwin-waitid-probe-r1'
out.mkdir()
rows = []
for status in (7,0):
    row = core.run_owned([sys.executable,'-c',f'raise SystemExit({status})'], out, out/f'exit-{status}.log', 5, dict(os.environ))
    rows.append(row)
    assert row['exit_code'] == status and row['owned_descendants_settled'] is True and row['cleanup_errors'] == [], row
    assert row['observed_exit'] == {'pid':row['owned_leader_pid'],'code':1,'status':status}, row
(out/'receipt.json').write_text(json.dumps(rows,indent=2))
print(json.dumps(rows,indent=2))
'''
result = subprocess.run(ssh + ['sh', '-c', "'cd \"$HOME/code/airk\" && /usr/bin/python3 -'"], input=code.encode(), capture_output=True, timeout=45)
receipt = {'source_sha256':seals,'exit_code':result.returncode,
           'stdout':result.stdout.decode(),'stderr':result.stderr.decode()}
Path('artifacts/advanced-split/receipts/delivery-03-neo-waitid-probe.json').write_text(json.dumps(receipt,indent=2))
print(result.stdout.decode())
print(result.stderr.decode())
raise SystemExit(result.returncode)
