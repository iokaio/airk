# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime, hashlib, importlib.util, json

root = Path.cwd()
a = root / 'artifacts/optimizations'
location = json.loads((a / 'f06-authority-runtime-location.json').read_text())
task = Path(location['task'])
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
assets = json.loads((a / 'cargo-clean-assets.json').read_text())
asset_count = 0
for folder, entries in assets.items():
    actual = {p.relative_to(root / 'target' / folder).as_posix() for p in (root / 'target' / folder).rglob('*') if p.is_file()}
    assert actual == set(entries)
    for name, expected in entries.items():
        path = root / 'target' / folder / name
        assert path.stat().st_size == expected['bytes'] and sha(path) == expected['sha256']
        asset_count += 1
preserved = json.loads((a / 'f06-primary-preserved-before-corrected-native.json').read_text())
for name, expected in preserved.items():
    path = Path(name)
    assert sha(path) == expected['sha256'] and path.stat().st_size == expected['bytes'], name
for item in location['scripts']:
    assert sha(task / 'scripts' / item['name']) == item['sha256']
fixtures = list((task / 'fixtures').iterdir())
assert not fixtures, 'owned fixtures remain'
spec = importlib.util.spec_from_file_location('f06_final_audit_builder', root / 'scripts/build-service-runtime.py')
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
source = builder.source_identity()
assert source == {'sha256': '5c4333f298c502844abb36a8eedf996ed6aa70fbbc8791876c899845024f5466', 'files': 708}
result = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'source': source,
          'offline_assets_exact_files_hashes_sizes': asset_count, 'preserved_old_evidence_files': len(preserved),
          'copied_script_hashes': len(location['scripts']), 'owned_fixture_entries': len(fixtures),
          'qualification_reports': {kind: json.loads((task / 'reports' / name / 'report.json').read_text())['cleanup']
                                    for kind, name in [('healthy', 'healthy-status-isolated-01'), ('scope', 'source-scope-01')]}}
(a / 'f06-authority-final-file-audit.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result, indent=2))
