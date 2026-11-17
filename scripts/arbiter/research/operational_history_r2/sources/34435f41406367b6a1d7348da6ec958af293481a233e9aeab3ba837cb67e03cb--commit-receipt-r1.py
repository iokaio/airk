# SPDX-License-Identifier: Apache-2.0
import hashlib, importlib.util, json, subprocess
from pathlib import Path
root = Path(__file__).resolve().parents[2]; base = Path(__file__).resolve().parent
def git(*args): return subprocess.check_output(['git', '-c', 'safe.directory=' + root.as_posix(), *args], cwd=root)
stage = json.loads((base / 'corrective-staged-r1.status.json').read_text(encoding='utf-8'))
message = git('log', '-1', '--format=%B').decode('utf-8')
assert 'Signed-off-by: Tyler Jensen <tyler@tsjensen.com>' in message.splitlines()
assert not git('status', '--porcelain=v1').strip()
assert {p.decode() for p in git('diff', '--name-only', '-z', 'HEAD^', 'HEAD').split(b'\0') if p} == set(stage['files'])
assert all(hashlib.sha256((root / p).read_bytes()).hexdigest() == digest for p, digest in stage['files'].items())
spec = importlib.util.spec_from_file_location('builder', root / 'scripts/build-service-runtime.py')
builder = importlib.util.module_from_spec(spec); spec.loader.exec_module(builder)
assert builder.source_identity() == stage['native_source']
value = {'commit': git('rev-parse', 'HEAD').decode().strip(), 'tree': git('rev-parse', 'HEAD^{tree}').decode().strip(),
    'message': message, 'native_source': stage['native_source'], 'public_files': len(stage['files']), 'worktree': 'clean',
    'qualification': 'Local DCO commit; tested source and all reviewed working bytes unchanged; no remote action.'}
with (base / 'local-commit-r1.json').open('x', encoding='utf-8') as stream: json.dump(value, stream, indent=2)
print(json.dumps(value))
