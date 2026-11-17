# SPDX-License-Identifier: Apache-2.0
"""Distinct CPU24 numeric preflight then large-model resource qualification."""
import datetime
import hashlib
import importlib.util
import json
import os
from pathlib import Path

root = Path.cwd()
out = root / 'artifacts/advanced-split/receipts'
binary = root / 'artifacts/v190/p10-target/debug/deps/airk_inference-cfc9134ebbe036bd.exe'
inputs = root / 'artifacts/advanced-split/models/native-weight8-mixed-cpu8-cpu24-profiles-r1/inputs.json'
def sha(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()
assert sha(inputs) == 'ae0f2bd9f19e374190cf0afdb2b86100f7d53933d0654801c1f812be5e616474'
spec = importlib.util.spec_from_file_location('cpu24_campaign_owner', root / 'scripts/provision-static-onnx-runtime.py')
owner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(owner)
record = out / 'delivery-06-cpu24-campaign-r1.json'
assert not record.exists()
receipt = {'scope': 'Windows isolated numeric parity at exact mixed smallCPU8/largeCPU24 tiers, then two fresh-process largeCPU24 T2048/N8/20-sample measurements; earlier unchanged smallCPU8 resource receipts remain separate',
           'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
           'binary': str(binary), 'binary_sha256': sha(binary),
           'input': str(inputs), 'input_sha256': sha(inputs),
           'profiles': [{**row, 'profile_sha256': sha(Path(row['profile'])), 'fixture_sha256': sha(Path(row['fixtures']))}
                        for row in json.loads(inputs.read_text())], 'runs': []}
record.write_text(json.dumps(receipt, indent=2))
prefix = 'inference_engine::pool::arbiter::qualification::'
runs = [('numeric-parity', prefix+'four_complete_trained_models_match_independent_compilers_and_raw_scores', None),
        ('resource-intern4', prefix+'complete_trained_model_maximum_context_has_twenty_native_warm_measurements', 'arbiter-large-intern-4b'),
        ('resource-kev4', prefix+'complete_trained_model_maximum_context_has_twenty_native_warm_measurements', 'arbiter-large-kev-4b')]
for name, selector, model in runs:
    artifact = out / f'delivery-06-cpu24-{name}-r1.json'
    log = out / f'delivery-06-cpu24-{name}-r1.log'
    assert not artifact.exists() and not log.exists()
    env = dict(os.environ, AIRK_ARBITER_QUALIFICATION_INPUT=str(inputs))
    if model:
        env.update(AIRK_ARBITER_RESOURCE_MODEL=model, AIRK_ARBITER_RESOURCE_OUTPUT=str(artifact))
    else:
        env['AIRK_ARBITER_QUALIFICATION_OUTPUT'] = str(artifact)
    result = owner.run_owned([str(binary), selector, '--ignored', '--exact', '--nocapture'], root, log, 3600, env)
    row = {'name': name, 'owned_result': result, 'artifact': str(artifact)}
    if artifact.exists():
        data = json.loads(artifact.read_text())
        row['artifact_sha256'] = sha(artifact)
        if model:
            ordered = sorted(data['samples_us'])
            row['postconditions'] = (data['model']==model and data['passed'] and len(ordered)==20
                and data['nearest_rank_p95_us']==ordered[18]<=30_000_000
                and 0<data['peak_process_rss_bytes']<=24<<30
                and len(data['input']['input_ids'])==2048 and len(data['input']['candidate_positions'])==8
                and data['execution_identity'][0]=='onnxruntime-cpu/1.30.0/physical24/intra24/inter1/sequential/spinning0')
            row.update(p95_us=data['nearest_rank_p95_us'], peak_process_rss_bytes=data['peak_process_rss_bytes'])
        else:
            row['postconditions'] = data['complete'] and len(data['models'])==4 and all(
                r['status']=='passed' and len(r['cases'])==3 and all(c['max_absolute_error']<=0.00025 for c in r['cases'])
                for r in data['models'])
    else:
        row['postconditions'] = False
    row['exact_one_test_passed'] = '1 passed; 0 failed' in log.read_text()
    receipt['runs'].append(row)
    receipt['binary_unchanged'] = sha(binary)==receipt['binary_sha256']
    receipt['input_unchanged'] = sha(inputs)==receipt['input_sha256']
    receipt['finished_through_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    record.write_text(json.dumps(receipt, indent=2))
    print(json.dumps({k:v for k,v in row.items() if k!='owned_result'}), flush=True)
    if not (owner.successful(result) and row['postconditions'] and row['exact_one_test_passed']
            and receipt['binary_unchanged'] and receipt['input_unchanged']):
        raise SystemExit(1)
receipt['complete'] = True
record.write_text(json.dumps(receipt, indent=2))

