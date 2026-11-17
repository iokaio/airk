# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tomllib

root = Path.cwd()
paths = [root / 'supply-chain' / name for name in ('config.toml', 'audits.toml', 'imports.lock')]
before = {p.name: p.read_bytes() for p in paths}
parsed = {name: tomllib.loads(data.decode()) for name, data in before.items()}
env = os.environ.copy()
env['PATH'] = os.pathsep.join([str(root / 'artifacts/tools/cargo-vet'), 'C:/Users/tdotn/.cargo/bin', env['PATH']])
result = subprocess.run(['C:/Users/tdotn/.cargo/bin/cargo.exe', 'vet', 'fmt'], env=env, capture_output=True, text=True)
after = {p.name: p.read_bytes() for p in paths}
assert {name: tomllib.loads(data.decode()) for name, data in after.items()} == parsed, 'Formatting changed policy semantics'
assert all(after[name] == before[name] for name in ('audits.toml', 'imports.lock')), 'Unrelated audit input changed'
report = {'exit_code': result.returncode, 'semantic_equality': True, 'before': {k: hashlib.sha256(v).hexdigest() for k,v in before.items()}, 'after': {k: hashlib.sha256(v).hexdigest() for k,v in after.items()}, 'stdout': result.stdout, 'stderr': result.stderr}
(root / 'artifacts/local/one-airk-1.8.14/vet-format-r1.json').write_text(json.dumps(report, indent=2)+'\n')
print(json.dumps(report))
raise SystemExit(result.returncode)
