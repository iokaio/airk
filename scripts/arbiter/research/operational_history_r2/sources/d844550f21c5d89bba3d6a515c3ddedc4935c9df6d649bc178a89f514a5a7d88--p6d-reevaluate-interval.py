# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
import json
from pathlib import Path

source = Path('scripts/check-service-source-fairness.py')
spec = importlib.util.spec_from_file_location('fairness', source)
fairness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fairness)
receipt = Path('artifacts/v190/p6d-source-fairness-release.json')
data = json.loads(receipt.read_text(encoding='utf-8'))
assert data['status'] == 'passed'
results = []
for wave in data['waves']:
    job = wave['source_job']['id']
    pending = next(item for item in wave['events'] if item['kind'] == 'job_pending' and item.get('job') == job)
    proof = fairness.admission_order(
        wave['events'], job, wave['order']['window_start_unix_ms'], wave['original_vector_job'],
        boundary_sequence=pending['sequence'] if wave['manual_watch_disabled'] else None,
        original_witness_unix_ms=wave['original_vector_observed_unix_ms'],
        original_witness_sequence_floor=wave['event_cursor_before_witness']['after'])
    assert proof['new_vector_overtakers'] == 0
    assert abs(proof['existing_turn_remaining_seconds'] - wave['order']['existing_turn_remaining_seconds']) < .000001
    results.append({'label': wave['label'], 'proof': proof})
result = {'status': 'passed', 'native_run_repeated': False,
          'original_receipt_sha256': hashlib.sha256(receipt.read_bytes()).hexdigest(),
          'validator_sha256': hashlib.sha256(source.read_bytes()).hexdigest(), 'waves': results}
Path('artifacts/v190/p6d-windows-interval-reevaluation.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
print(json.dumps({'status': result['status'], 'waves': len(results), 'native_run_repeated': False}))
