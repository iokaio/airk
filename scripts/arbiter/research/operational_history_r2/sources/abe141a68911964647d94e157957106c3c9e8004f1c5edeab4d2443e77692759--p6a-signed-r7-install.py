# SPDX-License-Identifier: Apache-2.0
"""Qualify installed signed R7 bytes using current bounded isolated fixtures."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import uuid

parent = Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh').resolve(strict=True)
checkout = parent / 'source-r2'
signed = checkout / 'artifacts/p6a-r7-gui-signed-0363da3003544eb2a3363140e3c20c78'
runtime = signed / 'runtime'
nonce = uuid.uuid4().hex
output = parent / f'artifacts/p6a-signed-r7-install-{nonce}'
log = output.with_suffix('.log')
receipt = output.with_suffix('.json')

def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

tools = [checkout / 'scripts' / name for name in ['check-service-install.py', 'check-client-lifetime.py',
    'build-service-runtime.py', 'sign-native.py', 'package-native.py']]
images = ['AIRK.app/Contents/MacOS/airk', 'AIRK.app/Contents/MacOS/airk-embed',
          'AIRK.app/Contents/MacOS/airk-launcher-image', 'AIRK.app/Contents/MacOS/libonnxruntime.dylib',
          'airk-install.json', 'airk-runtime.json', 'airk-build.json']
tools_before = {str(path.relative_to(checkout)): sha(path) for path in tools}
before = {name: sha(runtime / name) for name in images}
record = dict(schema='airk.private-signed-install-test/1', runtime=str(runtime), log=str(log),
              signing_receipt_sha256=sha(signed / 'signing.json'),
              provenance_scope='Actual signed R7 pair; current fixture scripts, not a fresh R5 native artifact',
              notarization_requested=False)
try:
    assert os.getuid() == 501
    build = json.loads((runtime / 'airk-build.json').read_text())
    assert build['source'] == dict(sha256='e261824ba5cf5e93286de5cee5a1b36a68cbc735990b1985bde50bae01ca6ad7', files=784)
    assert record['signing_receipt_sha256'] == '2de96f5e21f76e8bf4df7d8aeedae9b200a870de9ce739764d6ac11ff59a7a6b'
    assert before['AIRK.app/Contents/MacOS/airk-launcher-image'] == '6e781de8fad7cad3115ccbb57e98b20a3f334f791328c633bc7cc3ccc29616c7'
    record['runtime_source'] = build['source']
    command = [sys.executable, str(checkout / 'scripts/check-service-install.py'), '--runtime', str(runtime)]
    record['command'] = command
    started = time.monotonic()
    with log.open('x') as stream:
        result = subprocess.run(command, cwd=checkout, stdout=stream, stderr=subprocess.STDOUT, text=True, timeout=600)
    record.update(exit_code=result.returncode, elapsed_seconds=time.monotonic() - started)
    tail = log.read_text(errors='replace')[-8192:]
    record['log_tail'] = tail
    assert result.returncode == 0, 'Signed installed lifecycle fixture failed; preserve complete log'
    assert 'resumable uninstall passed.' in tail
    record['status'] = 'passed'
except BaseException as error:
    record.update(status='failed', failure=f'{type(error).__name__}: {error}')
    raise
finally:
    record['tools_before'] = tools_before
    record['tools_unchanged'] = tools_before == {str(path.relative_to(checkout)): sha(path) for path in tools}
    record['images_before'] = before
    record['signed_images_unchanged'] = before == {name: sha(runtime / name) for name in images}
    record['signing_receipt_unchanged'] = record['signing_receipt_sha256'] == sha(signed / 'signing.json')
    receipt.write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps(record, indent=2), flush=True)
