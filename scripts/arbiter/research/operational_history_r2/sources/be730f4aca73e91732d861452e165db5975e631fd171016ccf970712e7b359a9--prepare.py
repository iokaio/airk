# SPDX-License-Identifier: Apache-2.0
"""Generate ignored three-way draft only; never writes repository source."""
from pathlib import Path
import difflib
import hashlib
import json
import subprocess

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
PARENT = HERE.parent / 'f10b-revised'
FINAL = HERE.parent / 'f10b-recovery-owned-receipt'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def normalized(data):
    return data.decode('utf-8-sig').replace('\r\n', '\n').encode('utf-8')


def write(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


parent_map = json.loads((PARENT / 'BASE.json').read_text(encoding='utf-8'))['files']
final_map = json.loads((FINAL / 'SOURCE.json').read_text(encoding='utf-8'))['files']
paths = sorted(set(final_map) | {'crates/airk-cli/tests/integrity_restart.rs'})
base = {}
source = {}
decisions = []
conflicts = []
for name in paths:
    live = REPO / name
    old = PARENT / 'base' / name
    proposed = FINAL / 'source' / name
    actual = live.read_bytes() if live.exists() else None
    before = old.read_bytes() if old.exists() else None
    after = proposed.read_bytes() if proposed.exists() else None
    if name in final_map:
        assert after is not None and sha(after) == final_map[name], name
        identity = parent_map[name]
        if before is None:
            assert identity.get('absent'), name
        else:
            assert sha(before) == identity.get('effective_sha256', identity.get('sha256')), name
    else:
        before = actual
        after = actual
    base[name] = {'sha256': sha(actual)} if actual is not None else {'absent': True}
    if actual is not None:
        write(HERE / 'base' / name, actual)
    a, b, c = (normalized(v) if v is not None else None for v in (actual, before, after))
    if b == c:
        result, method = actual, 'preserve exact live; no F10b delta'
    elif a == b:
        result, method = c, 'apply reviewed F10b delta to unchanged effective base'
    elif a is None and b is None:
        result, method = c, 'new F10b source'
    elif a == c:
        result, method = actual, 'already exact reviewed value'
    else:
        assert all(v is not None for v in (a, b, c)), name
        scratch = HERE / 'merge-inputs' / name
        write(scratch.with_suffix(scratch.suffix + '.live'), a)
        write(scratch.with_suffix(scratch.suffix + '.base'), b)
        write(scratch.with_suffix(scratch.suffix + '.proposal'), c)
        merged = subprocess.run(['git', '-c', 'safe.directory=D:/code/Github/airk', 'merge-file', '-p',
                                 str(scratch.with_suffix(scratch.suffix + '.live')),
                                 str(scratch.with_suffix(scratch.suffix + '.base')),
                                 str(scratch.with_suffix(scratch.suffix + '.proposal'))], capture_output=True)
        assert 0 <= merged.returncode <= 127, (name, merged.stderr.decode(errors='replace'))
        result = merged.stdout
        method = 'normalized three-way merge; live compiler/platform/F11 corrections retained'
        if merged.returncode:
            conflicts.append(name)
            method = 'CONFLICT requires explicit reviewed resolution'
    assert result is not None, name
    write(HERE / 'source' / name, result)
    source[name] = sha(result)
    decisions.append({'path': name, 'method': method,
                      'live_changed_since_parent': a != b, 'f10b_delta': b != c})

manifest = {'schema': 1, 'kind': 'exact current tracked/worktree F11 raw snapshot; no adoption',
            'reported_native_seal': '927128a5eb90a71fb74ea53dac53a56b787ba3f86792d96ca3cfeb5d938344c5',
            'reported_native_inputs': 718,
            'head': subprocess.check_output(['git', '-c', 'safe.directory=D:/code/Github/airk', 'rev-parse', 'HEAD'], cwd=REPO).decode().strip(),
            'files': base}
write(HERE / 'BASE.json', (json.dumps(manifest, indent=2) + '\n').encode())
write(HERE / 'SOURCE.json', (json.dumps({'schema': 1, 'files': source}, indent=2) + '\n').encode())
write(HERE / 'MERGE-DECISIONS.json', (json.dumps({'conflicts': conflicts, 'files': decisions}, indent=2) + '\n').encode())
print(json.dumps({'files': len(paths), 'conflicts': conflicts}))
