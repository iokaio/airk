# SPDX-License-Identifier: Apache-2.0
import datetime
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import time

root = Path.cwd()
out = root / 'artifacts/advanced-split/receipts'
binary = root / 'artifacts/v190/p10-target/debug/deps/airk_inference-2769d8113a97de5e.exe'
inputs = root / 'artifacts/advanced-split/models/native-weight8-cpu8-qualification-profiles-r1/inputs.json'
def sha(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()
assert sha(inputs) == '2ccf7cb08202edb1cfe2d01267cc0462ec5fe20328f19a47cabe070d299f42c5'
spec = importlib.util.spec_from_file_location('resource_owned_runner', root / 'scripts/provision-static-onnx-runtime.py')
owner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(owner)
models = ['arbiter-small-intern-08b', 'arbiter-small-strands-2b', 'arbiter-large-intern-4b', 'arbiter-large-kev-4b']
source_profiles = [{**row, 'profile_sha256': sha(Path(row['profile'])), 'fixtures_sha256': sha(Path(row['fixtures']))} for row in json.loads(inputs.read_bytes())]
receipt = {'scope': 'Frozen passed native R3 binary; isolated CPU8/T2048/N8, one warmup and twenty warm samples per fresh process; no Cargo or concurrent compiler/model jobs',
    'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'binary': str(binary), 'binary_sha256': sha(binary),
    'input_sha256': sha(inputs), 'source_profiles': source_profiles, 'runs': []}
record = out / 'delivery-06-resource-campaign-r1.json'
assert not record.exists(), 'retain prior campaign evidence'
record.write_text(json.dumps(receipt, indent=2))
for index, model in enumerate(models):
    artifact = out / ('delivery-06-resource-' + model + '-r1.json')
    log = out / ('delivery-06-resource-' + model + '-r1.log')
    assert not artifact.exists() and not log.exists()
    env = dict(os.environ)
    env['AIRK_ARBITER_QUALIFICATION_INPUT'] = str(inputs)
    env['AIRK_ARBITER_RESOURCE_MODEL'] = model
    env['AIRK_ARBITER_RESOURCE_OUTPUT'] = str(artifact)
    argv = [str(binary), 'inference_engine::pool::arbiter::qualification::complete_trained_model_maximum_context_has_twenty_native_warm_measurements', '--ignored', '--exact', '--nocapture']
    started = time.monotonic()
    result = owner.run_owned(argv, root, log, 3600, env)
    row = {'model': model, 'argv': argv, 'elapsed_seconds': time.monotonic()-started, 'owned_result': result,
           'receipt': str(artifact) if artifact.exists() else None, 'receipt_sha256': sha(artifact) if artifact.exists() else None}
    receipt['runs'].append(row)
    receipt['binary_unchanged'] = sha(binary) == receipt['binary_sha256']
    receipt['finished_through_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    record.write_text(json.dumps(receipt, indent=2))
    print(json.dumps({'model': model, 'exit_code': result['exit_code'], 'elapsed_seconds': row['elapsed_seconds'], 'artifact': str(artifact)}), flush=True)
    if not owner.successful(result) or not receipt['binary_unchanged']:
        raise SystemExit(1)
receipt['complete'] = True
record.write_text(json.dumps(receipt, indent=2))
