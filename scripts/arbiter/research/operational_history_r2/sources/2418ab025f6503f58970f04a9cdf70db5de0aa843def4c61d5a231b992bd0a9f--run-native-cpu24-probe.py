# SPDX-License-Identifier: Apache-2.0
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
spec = importlib.util.spec_from_file_location('cpu24_probe_owned_runner', root / 'scripts/provision-static-onnx-runtime.py')
owner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(owner)
model = 'arbiter-large-intern-4b'
artifact = out / 'delivery-06-cpu24-intern4-developer-probe-r1.json'
log = out / 'delivery-06-cpu24-intern4-developer-probe-r1.log'
record = out / 'delivery-06-cpu24-intern4-developer-probe-run-r1.json'
assert not any(p.exists() for p in (artifact, log, record))
receipt = {'scope': 'Isolated one-warmup/three-observation CPU24 diagnostic only; no resource qualification',
           'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
           'binary': str(binary), 'binary_sha256': sha(binary),
           'input': str(inputs), 'input_sha256': sha(inputs)}
record.write_text(json.dumps(receipt, indent=2))
env = dict(os.environ, AIRK_ARBITER_QUALIFICATION_INPUT=str(inputs),
           AIRK_ARBITER_RESOURCE_MODEL=model, AIRK_ARBITER_RESOURCE_OUTPUT=str(artifact))
argv = [str(binary), 'inference_engine::pool::arbiter::qualification::large_cpu24_maximum_context_developer_probe_has_one_warmup_and_three_observations',
        '--ignored', '--exact', '--nocapture']
result = owner.run_owned(argv, root, log, 600, env)
receipt.update(owned_result=result, binary_unchanged=sha(binary)==receipt['binary_sha256'],
               input_unchanged=sha(inputs)==receipt['input_sha256'],
               finished_utc=datetime.datetime.now(datetime.timezone.utc).isoformat())
if artifact.exists():
    data = json.loads(artifact.read_text())
    receipt.update(artifact_sha256=sha(artifact), samples_us=data['samples_us'],
                   peak_process_rss_bytes=data['peak_process_rss_bytes'])
    assert data['developer_probe'] and not data['passed'] and data['nearest_rank_p95_us'] is None
    assert len(data['samples_us'])==3 and len(data['input']['input_ids'])==2048
record.write_text(json.dumps(receipt, indent=2))
print(json.dumps(receipt, indent=2))
raise SystemExit(0 if owner.successful(result) and receipt['binary_unchanged'] and receipt['input_unchanged'] and artifact.exists() else 1)

