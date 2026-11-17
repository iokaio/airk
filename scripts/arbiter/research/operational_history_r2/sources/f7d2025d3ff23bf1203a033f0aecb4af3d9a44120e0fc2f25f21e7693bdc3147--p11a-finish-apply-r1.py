# SPDX-License-Identifier: Apache-2.0
import hashlib, json, subprocess
from pathlib import Path
base = Path('D:/code/Github/airk/artifacts/v190')
work = base / 'p10-worktree'
root = base.parents[1]
def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
manifest = json.loads((base / 'p11a-accepted-p10-r2-base.json').read_text())
eol = []
for row in manifest['files']:
    path = work / row['path']
    if digest(path) != row['proposal_sha256']:
        proposed = base / 'p11a-accepted-p10-r2-proposed' / row['path']
        assert digest(proposed) == row['proposal_sha256']
        assert path.read_bytes().replace(b'\r\n', b'\n') == proposed.read_bytes().replace(b'\r\n', b'\n'), row['path']
        path.write_bytes(proposed.read_bytes())
        eol.append(row['path'])
    assert digest(path) == row['proposal_sha256']
support_base = base / 'p11a-effective-support-proposed'
support = json.loads((support_base / 'base.json').read_text())
for row in support['files']:
    assert digest(work / row['path']) == row['base_sha256'], row['path']
git = ['git', '-c', 'safe.directory=' + work.as_posix(), '-C', str(work)]
patch = support_base / 'effective-support.patch'
assert digest(patch) == '3c6cfc64277eed3be85f1ff31a17380fa9faa2eea0cf337e6dd343486473802e'
subprocess.run([*git, 'apply', '--check', '--ignore-space-change', str(patch)], check=True)
subprocess.run([*git, 'apply', '--ignore-space-change', str(patch)], check=True)
for row in support['files']:
    path = work / row['path']
    if digest(path) != row['proposal_sha256']:
        proposed = support_base / row['path']
        assert digest(proposed) == row['proposal_sha256']
        assert path.read_bytes().replace(b'\r\n', b'\n') == proposed.read_bytes().replace(b'\r\n', b'\n')
        path.write_bytes(proposed.read_bytes())
    assert digest(path) == row['proposal_sha256']
protected = ['crates/airk-store/src/current/postings.rs', 'crates/airk-store/src/current/postings_tests.rs',
    'crates/airk-kernel/src/split.rs', 'crates/airk-kernel/src/scoped_reads.rs', 'crates/airk-store/src/current/read_scope.rs',
    'docs/research/implementation-v190.md']
assert all((root / p).read_bytes() == (work / p).read_bytes() for p in protected)
output = base / 'p11a-applied-r1.json'
assert not output.exists()
output.write_text(json.dumps({'status':'applied, not compiled or accepted',
    'first_attempt':'First combined layer applied fully; exact hash check stopped on five line-ending-only Git patch outputs; all five verified LF-normalized identical and restored to exact reviewed bytes before applying support layer.',
    'line_ending_only_files':eol, 'protected_p10_unchanged':protected,
    'combined_patch':manifest['sha256'], 'support_patch':digest(patch)}, indent=2)+'\n')
print('P11a26files applied, exact reviewed bytes verified; all P10 protected owners unchanged')
