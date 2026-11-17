# SPDX-License-Identifier: Apache-2.0
import hashlib, importlib.util, json, subprocess
from pathlib import Path
root = Path(__file__).resolve().parents[2]
base = Path(__file__).resolve().parent
def git(*args):
    return subprocess.check_output(['git', '-c', 'safe.directory=' + root.as_posix(), *args], cwd=root)
def names(data): return {p.decode('utf-8') for p in data.split(b'\0') if p}
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
old = json.loads((base / 'corrective-final-r3.status.json').read_text(encoding='utf-8'))
paths = sorted(old['files'])
assert len(paths) == 50 and not names(git('diff', '--cached', '--name-only', '-z'))
assert names(git('diff', '--name-only', '-z', 'HEAD')) | names(git('ls-files', '--others', '--exclude-standard', '-z')) == set(paths)
docs = {'docs/reviews/branch-v190-review-2026-11-06.md', 'docs/releases/release-1.8.8.md', 'docs/research/implementation-v190.md'}
assert all(sha(root / p) == digest for p, digest in old['files'].items() if p not in docs)
assert git('config', 'user.name').decode().strip() == 'Tyler Jensen'
assert git('config', 'user.email').decode().strip() == 'tyler@tsjensen.com'
full = json.loads((base / 'windows-r1/full-native-r2.summary.json').read_text(encoding='utf-8'))
assert full['exit_code'] == 0 and full['passed'] == full['selected'] == 2681 and not full['failed']
gates = json.loads((base / 'windows-r1/prepackage-gates-r1.json').read_text(encoding='utf-8'))['results']
assert len(gates) == 6 and all(row['exit_code'] == 0 for row in gates)
spec = importlib.util.spec_from_file_location('builder', root / 'scripts/build-service-runtime.py')
builder = importlib.util.module_from_spec(spec); spec.loader.exec_module(builder)
source = json.loads((base / 'source-freeze-r3.json').read_text(encoding='utf-8'))['native_source']
assert builder.source_identity() == source
git('add', '--', *paths)
assert names(git('diff', '--cached', '--name-only', '-z')) == set(paths)
git('diff', '--cached', '--check')
patch = base / 'corrective-staged-r1.diff'
with patch.open('xb') as stream:
    subprocess.run(['git', '-c', 'safe.directory=' + root.as_posix(), 'diff', '--cached', '--binary', '--no-ext-diff'], cwd=root, stdout=stream, check=True)
assert builder.source_identity() == source
value = {'head': git('rev-parse', 'HEAD').decode().strip(), 'native_source': source,
    'files': {p: sha(root / p) for p in paths}, 'docs': {p: sha(root / p) for p in sorted(docs)},
    'diff_sha256': sha(patch), 'staged_whitespace': 'passed', 'qualification': 'Exactly 50 reviewed public files staged; no commit or remote action.'}
with (base / 'corrective-staged-r1.status.json').open('x', encoding='utf-8') as stream: json.dump(value, stream, indent=2)
print(json.dumps({'files': len(paths), 'native_source': source, 'docs': value['docs'], 'diff_sha256': value['diff_sha256']}))
