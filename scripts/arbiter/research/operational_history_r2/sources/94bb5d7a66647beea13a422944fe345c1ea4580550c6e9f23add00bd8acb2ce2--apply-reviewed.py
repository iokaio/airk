# SPDX-License-Identifier: Apache-2.0
"""Apply the reviewed public composition only while every preimage matches."""
from pathlib import Path
import hashlib
import json
import sys

root = Path(__file__).resolve().parents[2]
out = (root / sys.argv[1]).resolve()
assert out.is_relative_to(root / 'artifacts/qa-1.8.9')
manifest = json.loads((out / 'manifest.json').read_text(encoding='utf-8'))
assert manifest['status'] == 'composed'
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
for name, record in manifest['files'].items():
    relative = Path(name)
    assert not relative.is_absolute() and '..' not in relative.parts
    assert relative.parts[0] in {'crates', 'packages', 'docs', 'scripts'}
    destination = root / relative
    assert destination.resolve().is_relative_to(root.resolve())
    assert not destination.is_symlink() and not destination.is_junction()
    expected = record['original_sha256']
    assert (sha(destination) if destination.exists() else None) == expected, ('changed preimage', name)
    assert sha(out / 'tree' / relative) == record['proposed_sha256'], ('changed reviewed bytes', name)
backup = out / 'before-apply'
backup.mkdir()
for name, record in manifest['files'].items():
    destination = root / name
    source = out / 'tree' / name
    if destination.exists():
        saved = backup / name
        saved.parent.mkdir(parents=True, exist_ok=True)
        saved.write_bytes(destination.read_bytes())
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(source.read_bytes())
manifest['status'] = 'applied'
(out / 'applied-manifest.json').write_text(json.dumps(manifest, indent=2), encoding='utf-8')
print(json.dumps({'applied_files': len(manifest['files']), 'composition': str(out)}))
