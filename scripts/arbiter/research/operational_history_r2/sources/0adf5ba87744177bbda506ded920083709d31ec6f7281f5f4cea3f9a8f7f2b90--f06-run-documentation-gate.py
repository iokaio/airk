# SPDX-License-Identifier: Apache-2.0
"""Local F06 documentation/repository gate; no Cargo, native fixtures or publication."""
from pathlib import Path
import datetime
import hashlib
import json
import os
import subprocess
import sys
import time

root = Path.cwd()
artifacts = root / 'artifacts/optimizations'
tag = sys.argv[1]
assert tag in ['implementation', 'implementation-final', 'checkpoint']
output = artifacts / f'f06-d-{tag}.log'
receipt = artifacts / f'f06-d-{tag}.json'
assert not output.exists() and not receipt.exists()
env = os.environ.copy()
env.update(GIT_CONFIG_COUNT='1', GIT_CONFIG_KEY_0='safe.directory', GIT_CONFIG_VALUE_0='D:/code/Github/airk')
commands = [
    ['pwsh', '-NoProfile', '-File', 'scripts/check-repo.ps1'],
    [sys.executable, '-m', 'unittest', 'scripts.tests.test_ci_plan.PlanTests.test_docs_and_extension_content', '-v'],
    [sys.executable, 'scripts/release-version.py', 'check', '--expected', '1.8.0'],
    ['git', '-c', 'safe.directory=D:/code/Github/airk', 'diff', '--check'],
]
record = {'phase': 'F06a', 'cycle': tag, 'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'commands': []}
with output.open('x', encoding='utf-8') as log:
    for command in commands:
        log.write(json.dumps(command) + '\n')
        log.flush()
        start = time.monotonic()
        result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT)
        item = {'command': command, 'exit_code': result.returncode, 'seconds': time.monotonic() - start}
        record['commands'].append(item)
        receipt.write_text(json.dumps(record, indent=2) + '\n')
        print(json.dumps(item), flush=True)
        if result.returncode:
            raise SystemExit(result.returncode)
instructions = {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in ['AGENTS.md', 'CLAUDE.md']}
assert instructions['AGENTS.md'] == instructions['CLAUDE.md']
record.update(instruction_sha256=instructions, completed=True, ended_utc=datetime.datetime.now(datetime.timezone.utc).isoformat())
receipt.write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps({'complete': True, 'instruction_parity': True}))
