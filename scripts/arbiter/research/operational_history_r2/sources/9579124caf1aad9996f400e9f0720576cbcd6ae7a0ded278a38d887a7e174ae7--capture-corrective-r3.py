# SPDX-License-Identifier: Apache-2.0
import hashlib, json, subprocess
from pathlib import Path
root = Path(__file__).resolve().parents[2]
base = Path(__file__).resolve().parent
def git(*args):
    return subprocess.check_output(['git', '-c', 'safe.directory=' + root.as_posix(), *args], cwd=root)
tracked = [p.decode('utf-8') for p in git('diff', '--name-only', '-z', 'HEAD').split(b'\0') if p]
new = [p.decode('utf-8') for p in git('ls-files', '--others', '--exclude-standard', '-z').split(b'\0') if p]
assert len(tracked) == 44 and len(new) == 6, (tracked, new)
paths = sorted(tracked + new)
assert len(set(paths)) == 50 and all(not set(Path(p).parts) & {'temp', 'artifacts', '.airk', '.git'} for p in paths)
patch = base / 'corrective-final-r3.diff'
with patch.open('xb') as stream: subprocess.run(['git', '-c', 'safe.directory=' + root.as_posix(), 'diff', '--binary', '--no-ext-diff', 'HEAD', '--', *tracked], cwd=root, stdout=stream, check=True)
value = {'head': git('rev-parse', 'HEAD').decode().strip(), 'tracked': tracked, 'new': new,
    'files': {p: hashlib.sha256((root / p).read_bytes()).hexdigest() for p in paths},
    'diff_sha256': hashlib.sha256(patch.read_bytes()).hexdigest(), 'qualification': 'Current corrective snapshot; source frozen, documentation pending final gate collection.'}
with (base / 'corrective-final-r3.status.json').open('x', encoding='utf-8') as stream: json.dump(value, stream, indent=2)
print(json.dumps({'tracked': len(tracked), 'new': len(new), 'diff_sha256': value['diff_sha256']}))
