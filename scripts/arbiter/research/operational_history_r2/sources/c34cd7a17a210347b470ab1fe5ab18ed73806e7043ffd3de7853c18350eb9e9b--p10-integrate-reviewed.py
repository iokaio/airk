# SPDX-License-Identifier: Apache-2.0
import hashlib, importlib.util, json, subprocess, sys
from pathlib import Path

ROOT = Path('D:/code/Github/airk')
WORK = ROOT / 'artifacts/v190/p10-worktree'
BASE = 'b52017777b3df80c4b49adcaf324b1195f6bc9f8'
OWNED = {'AGENTS.md', 'CLAUDE.md', 'scripts/build-service-runtime.py'}

def git(where, *args):
    return subprocess.check_output(['git', '-c', f'safe.directory={where.as_posix()}',
        '-c', 'core.autocrlf=true', '-c', 'core.safecrlf=false', '-C', str(where), *args],
        stderr=subprocess.PIPE)

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.exists() else None

assert git(ROOT, 'rev-parse', 'HEAD').decode().strip() == BASE
assert git(WORK, 'rev-parse', 'HEAD').decode().strip() == BASE
changed = set(git(WORK, 'diff', '--name-only', '-z').decode().strip('\0').split('\0'))
new = set(git(WORK, 'ls-files', '--others', '--exclude-standard', '-z').decode().strip('\0').split('\0'))
paths = sorted((changed | new) - {''})
# Include raw-byte differences in tracked native inputs even when Git ignores
# line-ending-only overlays: release identity hashes actual bytes, not Git blobs.
native_args = ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo', 'crates',
    'scripts/build-service-runtime.py', 'scripts/provision-onnx-runtime.py',
    'scripts/provision-local-model.py', 'scripts/int8-policy.py', 'scripts/macho-identity.py',
    'scripts/license-notices.py', 'LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md', 'third_party']
native_names = git(WORK, 'ls-files', '-z', '--cached', '--others', '--exclude-standard', '--', *native_args)
byte_only = {p for p in native_names.decode().strip('\0').split('\0')
    if p and digest(WORK / p) != digest(ROOT / p)} - set(paths)
paths = sorted(set(paths) | byte_only)
assert paths and all(not p.startswith(('artifacts/', 'temp/', '.git/', '.airk/')) for p in paths)
root_dirty = set(git(ROOT, 'diff', '--name-only', '-z').decode().strip('\0').split('\0')) - {''}
assert root_dirty <= OWNED, root_dirty
rows = []
for name in paths:
    source, destination = WORK / name, ROOT / name
    assert source.is_file() and not source.is_symlink(), name
    if name in root_dirty:
        assert source.read_bytes() == destination.read_bytes(), name
    if name in new:
        assert not destination.exists(), name
    rows.append({'path': name, 'new': name in new, 'byte_only': name in byte_only,
        'before': digest(destination), 'tested': digest(source)})
qualification = json.loads((ROOT / 'artifacts/v190/p10-final-qualification-r6.json').read_text())
assert qualification['status'] == 'passed' and len(qualification['steps']) == 22
followup = json.loads((ROOT / 'artifacts/v190/p10-witness-correction-validation-r1.json').read_text())
assert followup['status'] == 'passed' and len(followup['steps']) == 7
neo_path = ROOT / 'artifacts/v190/p10-native-20261106-r4-witness-verification.json'
if neo_path.exists():
    neo = json.loads(neo_path.read_text())
    assert neo['status'] == 'passed' and neo['source'] == followup['source']
else:
    # Reversible integration may precede remote completion; acceptance/commit may not.
    assert (ROOT / 'artifacts/v190/p10-native-20261106-r4-active.json').is_file()
    snapshot = json.loads((ROOT / 'artifacts/v190/p10-native-20261106-r4-snapshot.json').read_text())
    assert snapshot['native_source'] == followup['source']
spec = importlib.util.spec_from_file_location('p10_builder', WORK / 'scripts/build-service-runtime.py')
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
assert builder.source_identity() == followup['source']
manifest = {'base': BASE, 'source': followup['source'], 'historical_full_qualification_source': qualification['source'], 'files': rows,
    'mac_verification_pending': not neo_path.exists(), 'phase_accepted': False,
    'excluded': 'All ignored state/artifacts/temp and non-native CRLF-only tracked differences; no deletions',
    'applied': False}
output = ROOT / ('artifacts/v190/p10-integration-manifest-r1.json' if '--apply' in sys.argv
    else 'artifacts/v190/p10-integration-preview-r2.json')
assert not output.exists(), 'Use a fresh immutable manifest label'
if '--apply' in sys.argv:
    for row in rows:
        source, destination = WORK / row['path'], ROOT / row['path']
        assert digest(source) == row['tested'] and digest(destination) == row['before']
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(source.read_bytes())
    assert all(digest(ROOT / row['path']) == row['tested'] for row in rows)
    spec = importlib.util.spec_from_file_location('root_builder', ROOT / 'scripts/build-service-runtime.py')
    integrated = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(integrated)
    assert integrated.source_identity() == followup['source']
    manifest['applied'] = True
output.write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
print(json.dumps({'files': len(rows), 'new': len(new), 'applied': manifest['applied'], 'source': manifest['source']}))
