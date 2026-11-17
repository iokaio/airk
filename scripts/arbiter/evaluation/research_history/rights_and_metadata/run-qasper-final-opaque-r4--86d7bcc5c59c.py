# SPDX-License-Identifier: Apache-2.0
"""Primary-owned bounded opaque alignment; never opens the emitted gold payload."""
import hashlib
import importlib.util
import json
from pathlib import Path
import sys

root = Path.cwd()
base = root / 'artifacts/advanced-split/models'
receipt_path = base / 'qasper-fresh-final-execution-r4.json'
if receipt_path.exists():
    raise SystemExit('Fresh execution receipt required')

def sha(path):
    with Path(path).open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()

review_path = base / 'qasper-fresh-final-opaque-review-r4.json'
assert sha(review_path) == '7bfb1467bb539a4c94359cbda0d90c7a48468042ddf72c76968f99e9efd21cab'
review = json.loads(review_path.read_text())
for name, expected in review.get('source_before_after_sha256', {}).items():
    assert sha(base / name) == expected, 'Reviewed source changed'
freeze_path = base / 'qasper-fresh-final-candidate-freeze-r4.json'
freeze_sha = 'ef657b29499f9d049d3479c47c7188cce307ba901c658958464df4239ccc9152'
assert sha(freeze_path) == freeze_sha
freeze = json.loads(freeze_path.read_text())
assert len(freeze['input_seals']) == 109
for item in review['protected_roots_actual_identity']:
    path = root / item['relative']
    assert path.resolve(strict=True).is_dir()
    assert str(path.resolve()) == item['resolved']
    assert (path.stat().st_dev,path.stat().st_ino) == (item['device'],item['inode'])
before = {}
for name, expected in freeze['input_seals'].items():
    path = root / name
    assert sha(path) == expected, 'Frozen input changed'
    before[name] = expected

proof_path = base / 'qasper-final-anchor-actual-r4.json'
proof = json.loads(proof_path.read_text(encoding='utf-8-sig'))
assert proof['current_owner_only'] and proof['protected'] and proof['no_reparse']
assert proof['current_sid'] == proof['owner_sid']
rules = proof['rules'] if isinstance(proof['rules'], list) else [proof['rules']]
assert all(rule['sid'] == proof['current_sid'] and rule['type'] == 'Allow' for rule in rules)
assert any(rule['propagation'] == 'None' and 'FullControl' in rule['rights']
           and 'ContainerInherit' in rule['inheritance'] and 'ObjectInherit' in rule['inheritance']
           for rule in rules)
anchor = base / 'qasper-fresh-final-opaque-anchor-r4'
assert Path(proof['canonical_path']).samefile(anchor)
assert not list(anchor.iterdir()), 'Fresh empty anchor required'

job_source = root / 'scripts/provision-static-onnx-runtime.py'
job_sha = sha(job_source)
spec = importlib.util.spec_from_file_location('owned_qasper_alignment', job_source)
job = importlib.util.module_from_spec(spec)
spec.loader.exec_module(job)
argv = [sys.executable, str(base / 'qasper-fresh-final-materializer-r4.py'),
        '--freeze', str(freeze_path), '--freeze-sha256', freeze_sha,
        '--raw-root', str(base / 'qasper-rights-audit-v03'),
        '--output', str(anchor / 'materialized'), '--execute-reviewed-content-alignment']
result = job.run_owned(argv, root, base / 'qasper-fresh-final-execution-r4.log', 60)
after = {name: sha(root / name) for name in before}
receipt = {'schema': 'airk.private-qasper-final-primary-execution/1',
           'review_sha256': sha(review_path), 'freeze_sha256': freeze_sha,
           'anchor_proof_sha256': sha(proof_path), 'owned_observer_seconds': 60,
           'primary_runner_sha256': sha(__file__), 'job_helper_sha256': job_sha, 'job_helper_unchanged': sha(job_source) == job_sha,
           'input_count': len(before), 'inputs_unchanged': before == after,
           'changed_inputs': [name for name in before if before[name] != after[name]],
           'owned_result': result, 'gold_opened_by_observer': False}
report_path = anchor / 'materialized/opaque-counts.json'
if report_path.exists():
    report = json.loads(report_path.read_text())
    receipt['aggregate'] = {key: report[key] for key in
        ('complete', 'minimum_met', 'families', 'raw_questions', 'eligible',
         'selected', 'refusal_counts', 'sealed_payload_sha256')}
    receipt['opaque_report_sha256'] = sha(report_path)
with receipt_path.open('x') as output:
    json.dump(receipt, output, indent=2)
print(json.dumps({key: value for key, value in receipt.items() if key != 'owned_result'}))
print(json.dumps(result))
if result.get('exit_code') != 0 or result.get('owned_job_forced_termination') or not result.get('owned_descendants_settled') or result.get('timed_out') or result.get('log_oversized') or not receipt['inputs_unchanged'] or not receipt['job_helper_unchanged']:
    raise SystemExit(1)
