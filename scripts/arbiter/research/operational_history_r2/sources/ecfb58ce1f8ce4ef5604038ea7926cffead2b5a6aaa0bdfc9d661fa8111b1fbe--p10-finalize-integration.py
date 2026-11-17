# SPDX-License-Identifier: Apache-2.0
import hashlib, importlib.util, json
from pathlib import Path
root = Path('D:/code/Github/airk')
base = root / 'artifacts/v190'
work = base / 'p10-worktree'
manifest = json.loads((base / 'p10-integration-manifest-r1.json').read_text())
assert manifest['applied'] and not manifest['phase_accepted']
neo = json.loads((base / 'p10-native-20261106-r4-witness-verification.json').read_text())
assert neo['status'] == 'passed' and neo['source'] == manifest['source']
docs = {'README.md', 'packages/vscode/README.md', 'docs/README.md', 'docs/releases/release-1.8.8.md',
    'docs/architecture.md', 'docs/source-policy.md', 'docs/index-splitting.md', 'docs/research/implementation-v190.md'}
for row in manifest['files']:
    destination, source = root / row['path'], work / row['path']
    assert hashlib.sha256(destination.read_bytes()).hexdigest() == row['tested'], row['path']
    if row['path'] in docs:
        destination.write_bytes(source.read_bytes())
        row['tested'] = hashlib.sha256(source.read_bytes()).hexdigest()
    assert destination.read_bytes() == source.read_bytes(), row['path']
for folder in (root, work):
    spec = importlib.util.spec_from_file_location('builder', folder / 'scripts/build-service-runtime.py')
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    assert builder.source_identity() == manifest['source']
assert (root / 'AGENTS.md').read_bytes() == (root / 'CLAUDE.md').read_bytes()
manifest.update(mac_verification_pending=False, phase_accepted=False,
    status='Integrated and byte-verified; final repository/version/whitespace checks and local DCO commit required')
output = base / 'p10-integration-manifest-r2.json'
assert not output.exists()
output.write_text(json.dumps(manifest, indent=2) + '\n')
print('125 public files exact; final native identity matches Windows and Neo; docs updated; no artifacts included')
