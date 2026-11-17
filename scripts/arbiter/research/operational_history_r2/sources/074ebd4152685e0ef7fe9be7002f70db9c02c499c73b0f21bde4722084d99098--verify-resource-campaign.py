# SPDX-License-Identifier: Apache-2.0
"""Read-only postcondition audit of the frozen four-model native campaign."""
import hashlib
import json
from pathlib import Path

base = Path('artifacts/advanced-split/receipts')
campaign_path = base / 'delivery-06-resource-campaign-r1.json'
campaign = json.loads(campaign_path.read_text())
sha = lambda p: hashlib.sha256(Path(p).read_bytes()).hexdigest()
models = ['arbiter-small-intern-08b', 'arbiter-small-strands-2b',
          'arbiter-large-intern-4b', 'arbiter-large-kev-4b']
assert campaign['binary_unchanged'] and sha(campaign['binary']) == campaign['binary_sha256']
assert [run['model'] for run in campaign['runs']] == models, 'campaign incomplete'
rows = []
for run, source in zip(campaign['runs'], campaign['source_profiles'], strict=True):
    owned = run['owned_result']
    assert owned['exit_code'] == 0 and not owned['timed_out']
    assert owned['owned_descendants_settled'] and not owned['owned_job_forced_termination']
    assert sha(run['receipt']) == run['receipt_sha256']
    assert sha(source['profile']) == source['profile_sha256']
    assert sha(source['fixtures']) == source['fixtures_sha256']
    profile = json.loads(Path(source['profile']).read_text())
    receipt = json.loads(Path(run['receipt']).read_text())
    assert receipt['model'] == run['model'] == profile['model']
    assert receipt['profile'] == profile['manifest']
    assert receipt['profile_source_sha256'] == source['profile_sha256']
    assert receipt['schema'] == 'airk.arbiter.native-resource/1'
    assert receipt['evaluation'] and not receipt['release_qualified'] and receipt['passed']
    assert receipt['execution_identity'][0] == 'onnxruntime-cpu/1.30.0/physical8/intra8/inter1/sequential/spinning0'
    assert receipt['execution_identity'][1] >= 8
    compiled = receipt['input']
    assert all(len(compiled[key]) == 2048 for key in ('input_ids', 'attention_mask', 'position_ids'))
    assert all(len(compiled[key]) == 8 for key in ('candidate_positions', 'option_mask'))
    samples = receipt['samples_us']
    assert len(samples) == 20 and all(type(sample) is int and sample > 0 for sample in samples)
    assert receipt['nearest_rank_p95_us'] == sorted(samples)[18] <= 30_000_000
    assert receipt['maximum_us'] == max(samples)
    ceiling = (12 if 'small' in run['model'] else 24) << 30
    assert receipt['rss_ceiling_bytes'] == ceiling
    assert 0 < receipt['peak_process_rss_bytes'] <= ceiling
    log = Path(owned['log']).read_text()
    assert 'running 1 test' in log and '1 passed; 0 failed' in log
    rows.append({'model': run['model'], 'samples': 20,
                 'p95_seconds': receipt['nearest_rank_p95_us'] / 1e6,
                 'peak_gib': receipt['peak_process_rss_bytes'] / (1 << 30),
                 'receipt_sha256': run['receipt_sha256']})
print(json.dumps({'scope': 'All four exact native CPU8 resource receipts only; no quality, product lifecycle or release claim',
                  'campaign_sha256': sha(campaign_path), 'models': rows}, indent=2))
