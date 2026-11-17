# SPDX-License-Identifier: Apache-2.0
import hashlib, json
from pathlib import Path

base = Path('D:/code/Github/airk/artifacts/v190')
campaign = json.loads((base / 'p10-final-qualification-r6.json').read_text())
assert campaign['status'] == 'passed' and campaign['source_gates_complete']
rows = []
for step in campaign['steps']:
    receipt_path = Path(step['receipt'])
    receipt = json.loads(receipt_path.read_text())
    assert step['exit_code'] == receipt['exit_code'] == receipt['actual_exit_code'] == 0
    assert receipt['source_before'] == receipt['source_after'] == campaign['source']
    assert receipt['source_unchanged'] and receipt['validation_error'] is None
    log = receipt_path.with_name(receipt_path.name.replace('.receipt.json', '.log'))
    assert hashlib.sha256(log.read_bytes()).hexdigest() == receipt['log_sha256'], step['name']
    rows.append({'stage': step['name'], 'log_sha256': receipt['log_sha256'],
        'seconds': receipt['seconds'], 'passing_tests': receipt['passing_tests']})
output = base / 'p10-primary-final-receipt-verification-r1.json'
assert not output.exists()
output.write_text(json.dumps({'status': 'passed', 'source': campaign['source'],
    'stages': rows}, indent=2) + '\n')
print(json.dumps({'status': 'passed', 'verified_stages': len(rows), 'source': campaign['source']}))
