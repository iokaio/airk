# SPDX-License-Identifier: Apache-2.0
import hashlib, json, subprocess
from pathlib import Path
base = Path('D:/code/Github/airk/artifacts/v190')
work = base / 'p10-worktree'
git = ['git', '-c', 'safe.directory=' + work.as_posix(), '-C', str(work)]
assert subprocess.check_output([*git, 'rev-parse', 'HEAD']).decode().strip() == 'c2584cb05dfdeba6e948b91e4964479a07f9d3fc'
def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.exists() else None
layers = [('p11a-accepted-p10-r2.proposed.patch', 'p11a-accepted-p10-r2-base.json',
    '91ff250d574e08d43352e419aba762732c5720ad2f142412700c59c77f68383c'),
    ('p11a-effective-support-proposed/effective-support.patch', 'p11a-effective-support-proposed/base.json',
    '3c6cfc64277eed3be85f1ff31a17380fa9faa2eea0cf337e6dd343486473802e')]
protected = ['crates/airk-store/src/current/postings.rs', 'crates/airk-store/src/current/postings_tests.rs',
    'crates/airk-kernel/src/split.rs', 'crates/airk-kernel/src/scoped_reads.rs', 'crates/airk-store/src/current/read_scope.rs',
    'docs/research/implementation-v190.md']
before = {p: digest(work / p) for p in protected}
applied = []
for patch_name, manifest_name, expected in layers:
    patch = base / patch_name
    manifest = json.loads((base / manifest_name).read_text())
    assert digest(patch) == expected
    for row in manifest['files']:
        assert digest(work / row['path']) == row['base_sha256'], row['path']
    subprocess.run([*git, 'apply', '--check', '--ignore-space-change', str(patch)], check=True)
    subprocess.run([*git, 'apply', '--ignore-space-change', str(patch)], check=True)
    for row in manifest['files']:
        assert digest(work / row['path']) == row['proposal_sha256'], row['path']
    applied.append({'patch': patch_name, 'sha256': expected, 'files': len(manifest['files'])})
assert before == {p: digest(work / p) for p in protected}
output = base / 'p11a-applied-r1.json'
assert not output.exists()
output.write_text(json.dumps({'status': 'applied, not compiled or accepted', 'layers': applied,
    'protected_unchanged': before}, indent=2) + '\n')
print('P11a applied:26 public files; protected P10 scope/witness/ledger unchanged')
