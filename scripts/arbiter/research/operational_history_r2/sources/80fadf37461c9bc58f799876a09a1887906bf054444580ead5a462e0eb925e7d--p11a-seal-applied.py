# SPDX-License-Identifier: Apache-2.0
import hashlib, json
from pathlib import Path
base = Path('D:/code/Github/airk/artifacts/v190')
work, root = base / 'p10-worktree', base.parents[1]
combined = json.loads((base / 'p11a-accepted-p10-r2-base.json').read_text())
support = json.loads((base / 'p11a-effective-support-proposed/base.json').read_text())
expected = {r['path']:(r,base/'p11a-accepted-p10-r2-proposed'/r['path']) for r in combined['files']}
expected.update({r['path']:(r,base/'p11a-effective-support-proposed/source'/r['path']) for r in support['files']})
normalized = []
for name,(row,source) in expected.items():
    target = work / name
    proposed = source.read_bytes()
    assert hashlib.sha256(proposed).hexdigest() == row['proposal_sha256']
    assert target.read_bytes().replace(b'\r\n',b'\n') == proposed.replace(b'\r\n',b'\n'), name
for name,(row,source) in expected.items():
    target = work / name
    if target.read_bytes() != source.read_bytes():
        target.write_bytes(source.read_bytes())
        normalized.append(name)
    assert hashlib.sha256(target.read_bytes()).hexdigest() == row['proposal_sha256']
protected = ['crates/airk-store/src/current/postings.rs','crates/airk-store/src/current/postings_tests.rs',
    'crates/airk-kernel/src/split.rs','crates/airk-kernel/src/scoped_reads.rs','crates/airk-store/src/current/read_scope.rs',
    'docs/research/implementation-v190.md']
assert all((root/p).read_bytes() == (work/p).read_bytes() for p in protected)
output = base/'p11a-applied-r1.json'
assert not output.exists()
output.write_text(json.dumps({'status':'26files applied and exact reviewed bytes sealed; not compiled or accepted',
    'combined_patch':combined['sha256'],'support_patch':'3c6cfc64277eed3be85f1ff31a17380fa9faa2eea0cf337e6dd343486473802e',
    'application_notes':'Both Git patches applied. Exact hash checks detected line-ending-only differences in five combined-layer files and Ingest support-layer file; all were verified LF-normalized identical before copying exact reviewed bytes. Historical application attempts remain in tool output.',
    'final_line_ending_fixups':normalized,'protected_unchanged':protected},indent=2)+'\n')
print('All26 P11a files match exact reviewed bytes; protected P10 files unchanged')
