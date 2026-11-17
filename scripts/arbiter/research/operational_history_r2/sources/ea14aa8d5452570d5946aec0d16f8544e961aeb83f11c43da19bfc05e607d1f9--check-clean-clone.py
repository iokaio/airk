# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parents[2]
base = Path(__file__).resolve().parent
catalog_path = root / 'tools/parser-train/data/catalog.json'
catalog_bytes = catalog_path.read_bytes()
catalog = json.loads(catalog_bytes)
files = {entry['path'] for entry in catalog['source_files'] + catalog['license_files'] + catalog['datasets']}
files.update({'LICENSE', '.gitignore', 'tools/parser-train/data/README.md',
              'tools/parser-train/data/catalog.json', 'crates/airk-retrieval/assets/intent-v1.bin'})
with tempfile.TemporaryDirectory(prefix='branch-only-restore-', dir=base) as directory:
    clone = Path(directory).resolve()
    assert clone.is_relative_to(base.resolve())
    for name in sorted(files):
        destination = clone / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(root / name, destination)
    assert not (clone / 'target').exists()
    assert not (clone / '.parser-train').exists()
    assert not (clone / 'target/parser-train/repos').exists()
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
    proof = {'schema': 1, 'catalog_sha256': hashlib.sha256(catalog_bytes).hexdigest(),
             'current_model': catalog['current_model'], 'totals': catalog['totals'],
             'branch_files_copied': len(files), 'restored_exact_paths': restored,
             'outcomes': outcomes,
             'scope': 'Temporary copy of branch-visible source/fixtures/model/gzip/licenses only; no ignored raw examples, clones, vocabulary or native state. verify then CLI restore passed and every restored JSONL digest matched. Temporary copy removed after validation.'}
    (base / 'restore-proof.json').write_text(json.dumps(proof, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'branch_only_restore': 'passed', 'restored_exact_paths': restored,
                      'catalog_sha256': proof['catalog_sha256'], **catalog['totals']}))
