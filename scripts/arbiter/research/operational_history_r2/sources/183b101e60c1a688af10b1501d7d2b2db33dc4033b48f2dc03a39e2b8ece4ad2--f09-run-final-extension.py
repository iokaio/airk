# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime, importlib.util, json, os, subprocess, sys, time

root = Path.cwd()
artifacts = root / 'artifacts/optimizations'
prefix='f09-final-corrected-full-e' if sys.argv[1:]==['corrected'] else 'f09-final-full-e'
spec = importlib.util.spec_from_file_location('f09_e_builder', root / 'scripts/build-service-runtime.py')
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
env = os.environ.copy()
env.pop('AIRK_TEST_BINARY', None)
env.pop('AIRK_TEST_F09_QUALIFICATION', None)
command = ['npm.cmd', 'test']
record = {
    'command': command, 'cwd': str(root / 'packages/vscode'),
    'AIRK_TEST_BINARY': 'unset', 'AIRK_TEST_F09_QUALIFICATION':'unset', 'source_before': builder.source_identity(),
    'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
}
receipt = artifacts / f'{prefix}-execution.json'
with receipt.open('x',encoding='utf-8') as stream:
    stream.write(json.dumps(record, indent=2) + '\n')
started = time.monotonic()
with (artifacts / f'{prefix}.log').open('x', encoding='utf-8') as output:
    result = subprocess.run(command, cwd=root / 'packages/vscode', env=env, stdout=output, stderr=subprocess.STDOUT)
record.update(exit_code=result.returncode, elapsed_seconds=time.monotonic() - started,
              ended_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(), source_after=builder.source_identity())
record['source_unchanged'] = record['source_before'] == record['source_after']
receipt.write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps(record), flush=True)
raise SystemExit(result.returncode or (0 if record['source_unchanged'] else 1))
