# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
from pathlib import Path

base = Path('D:/code/Github/airk/artifacts/v190')
stem = 'p14-editor-failed-fixture-'
proofs = {}
for kind in ['repair', 'inspect']:
    path = base / (stem + kind + '-r1.receipt.json')
    receipt = json.loads(path.read_text())
    log = base / (stem + kind + '-r1.log')
    assert receipt['exit_code'] == receipt['actual_exit_code'] == 0 and receipt['source_unchanged'] is True
    assert hashlib.sha256(log.read_bytes()).hexdigest() == receipt['log_sha256']
    proofs[kind] = {'receipt': str(path), 'log_sha256': receipt['log_sha256'], 'result': json.loads(log.read_text())}
runtime = '45da6d3a10b34313dea53881ce95a930f89aa304f019ba095406eb40b852085c'
assert proofs['repair']['result'] == {'launcher_repaired': True, 'runtime': runtime}
inspection = proofs['inspect']['result']
assert inspection['approval']['manifest_sha256'] == runtime and inspection['payload_verified'] is True
assert inspection['registry_namespace'] == 'a007ecd0e1b3cba776ff5d9d021adabf'
assert inspection['lifecycle']['pending'] is None and inspection['state_error'] is None
result = {'qualified': True, 'scope': 'owned failed editor fixture, native cleanup only',
          'proof': 'native repair-launcher acquired actual service/project/helper/watch quiescence leases after natural service exit',
          'evidence': proofs, 'lifecycle_stop_claimed': False, 'ui_stop_receipt_created': False,
          'original_editor_result': 'failed, unchanged',
          'prior_attempt': 'Fresh health refused before any Stop dispatch; no restart, force or PID kill'}
with (base / 'p14-editor-failed-fixture-cleanup-r2.json').open('x') as output:
    json.dump(result, output, indent=2)
print(json.dumps({'qualified': True, 'proof': result['proof']}))
