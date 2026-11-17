# SPDX-License-Identifier: Apache-2.0
from pathlib import Path, PurePosixPath
import hashlib
import json
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parents[2]
base = Path(__file__).resolve().parent
def git(*arguments):
    return subprocess.run(['git', '-c', 'safe.directory=' + root.as_posix(), *arguments],
                          cwd=root, check=True, capture_output=True).stdout

def index_entries():
    rows = {}
    for entry in git('ls-files', '--stage', '-z').split(b'\0'):
        if not entry:
            continue
        fields, name = entry.split(b'\t', 1)
        mode, oid, stage = fields.decode().split()
        name = name.decode('utf-8')
        if stage != '0':
            raise ValueError('Unmerged staged file cannot qualify: ' + name)
        rows[name] = (mode, oid)
    return rows

index = index_entries()
catalog_name = 'tools/parser-train/data/catalog.json'
catalog_bytes = git('cat-file', 'blob', index[catalog_name][1])
catalog = json.loads(catalog_bytes)
files = {entry['path'] for entry in catalog['source_files'] + catalog['license_files'] + catalog['datasets']}
files.update({'LICENSE', '.gitignore', '.gitattributes', 'tools/parser-train/.gitattributes',
              'tools/parser-train/data/README.md', catalog_name, 'crates/airk-retrieval/assets/intent-v1.bin'})
attributes = [name for name in files if name.startswith('tools/parser-train/')
              and (name.endswith('.json') or name.endswith('.gz') or '/data/licenses/' in name and name.endswith('.txt'))]
raw_attributes = git('check-attr', '--cached', '-z', 'text', 'whitespace', 'diff', '--', *sorted(attributes)).split(b'\0')
checks = {}
for offset in range(0, len(raw_attributes) - 1, 3):
    name, key, value = (part.decode('utf-8') for part in raw_attributes[offset:offset + 3])
    checks.setdefault(name, {})[key] = value
for name, values in checks.items():
    assert values['text'] == 'unset', (name, values)
    assert values['diff'] == 'unspecified', (name, values)
    if not name.endswith('.gz'):
        assert values['whitespace'] == 'blank-at-eol,blank-at-eof,space-before-tab,cr-at-eol', (name, values)

with tempfile.TemporaryDirectory(prefix='staged-branch-restore-', dir=base) as directory:
    clone = Path(directory).resolve()
    assert clone.is_relative_to(base.resolve())
    selected_oids = {}
    for name in sorted(files):
        path = PurePosixPath(name)
        assert not path.is_absolute() and '..' not in path.parts and '\\' not in name
        mode, oid = index[name]
        assert mode in ('100644', '100755'), (name, mode)
        content = git('cat-file', 'blob', oid)
        destination = clone / name
        assert destination.resolve().is_relative_to(clone)
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(content)
        selected_oids[name] = oid
    assert not (clone / 'target').exists()
    assert not (clone / '.parser-train').exists()
    outcomes = {}
    for action in ('verify', 'restore'):
        result = subprocess.run([sys.executable, 'tools/parser-train/export_datasets.py', action],
                                cwd=clone, check=True, capture_output=True, text=True)
        outcomes[action] = json.loads(result.stdout)
    restored = 0
    for entry in catalog['datasets']:
        for name in entry['original_paths']:
            content = (clone / name).read_bytes()
            assert len(content) == entry['raw_bytes']
            assert hashlib.sha256(content).hexdigest() == entry['raw_sha256']
            restored += 1
    final_index = index_entries()
    assert all(final_index[name][1] == oid for name, oid in selected_oids.items()), 'Selected staged blobs changed during proof'
    proof = {'schema': 1, 'catalog_sha256': hashlib.sha256(catalog_bytes).hexdigest(),
             'current_model': catalog['current_model'], 'totals': catalog['totals'],
             'staged_files_extracted': len(files), 'restored_exact_paths': restored,
             'staged_blob_oids': selected_oids, 'cached_attribute_checks': checks,
             'outcomes': outcomes,
             'scope': 'Exact staged Git blobs extracted to a temporary isolated root, with cached Git attributes checked. No worktree source copies or ignored raw examples/clones/vocabulary/native state were used. CLI verify and restore passed; every restored JSONL digest matched. Temporary root removed.'}
    (base / 'staged-restore-proof.json').write_text(json.dumps(proof, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'staged_git_restore': 'passed', 'restored_exact_paths': restored,
                      'catalog_sha256': proof['catalog_sha256'], 'staged_files_extracted': len(files),
                      'attribute_paths_checked': len(checks), **catalog['totals']}))
