# SPDX-License-Identifier: Apache-2.0
"""Own the actual private Service test fixture; never fabricate Ready/closed."""
import datetime
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil

root = Path.cwd()
source = root / 'artifacts/v190/p10-target/debug/deps/airk_service-82bc4828aacb3e91.exe'
home = root / 'artifacts/advanced-split/native-harnesses/goal-fixture-cpu24-r4'
home.mkdir(parents=True, exist_ok=False)
def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()
original = sha(source)
binary = home / source.name
shutil.copy2(source, binary)
assert sha(binary) == original == sha(source)
inputs = root / 'artifacts/advanced-split/models/native-hybrid-next-read-profiles-cpu24-r2/materialized/evaluation-inputs.json'
assert sha(inputs) == '868810174d6c287502653d49ded5bddbfe3d1b8c43519bd97ae023c4797fd3d4'
work = root / 'artifacts/advanced-split/models/native-goal-private-anchor-r4/work'
assert not work.exists()
record = {'scope': 'Actual private Service/lease/listener/evaluation owner. Experimental encoder failed A1; no production admission or release qualification.',
          'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'binary': str(binary), 'binary_sha256': original,
          'input': str(inputs), 'input_sha256': sha(inputs), 'work': str(work),
          'fixture_lifetime_seconds': 9600}
(home / 'run.json').write_text(json.dumps(record, indent=2))
spec = importlib.util.spec_from_file_location('goal_fixture_owned_observer', root / 'scripts/provision-static-onnx-runtime.py')
owner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(owner)
env = dict(os.environ, AIRK_ARBITER_EVALUATION_INPUTS=str(inputs),
           AIRK_ARBITER_EVALUATION_INPUTS_SHA256=record['input_sha256'],
           AIRK_ARBITER_EVALUATION_WORK=str(work), AIRK_ARBITER_EVALUATION_SECONDS='9600')
result = owner.run_owned([str(binary), 'service_runtime::evaluation_fixture_tests::actual_private_arbiter_fixture',
                          '--ignored', '--exact', '--nocapture'], root, home/'native.log', 10800, env)
record.update(owned_result=result, finished_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
              binary_unchanged=sha(binary)==original, input_unchanged=sha(inputs)==record['input_sha256'])
(home / 'run.json').write_text(json.dumps(record, indent=2))
print(json.dumps(record, indent=2))
raise SystemExit(0 if owner.successful(result) and record['binary_unchanged'] and record['input_unchanged'] else 1)

