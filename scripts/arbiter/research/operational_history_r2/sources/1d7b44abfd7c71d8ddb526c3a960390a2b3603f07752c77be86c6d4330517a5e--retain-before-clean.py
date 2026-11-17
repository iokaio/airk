# SPDX-License-Identifier: Apache-2.0
"""Retain identified native harnesses/evidence before authorized Cargo cleanup."""
import datetime
import hashlib
import json
from pathlib import Path
import shutil

root = Path.cwd().resolve()
target = (root / 'artifacts/v190/p10-target').resolve(strict=True)
assert target == root / 'artifacts/v190/p10-target' and target.is_relative_to(root)
destination = root / 'artifacts/advanced-split/native-harnesses/delivery-06-before-clean'
destination.mkdir(parents=True, exist_ok=False)
def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()
names = ['airk_inference-b282fdd34c3b77f6.exe', 'airk_kernel-43b1554a9b9060c4.exe',
         'airk_service-6639699f92708d81.exe']
rows = []
for name in names:
    source = target / 'debug/deps' / name
    original_hash = sha(source)
    retained = destination / name
    shutil.copy2(source, retained)
    assert sha(retained) == original_hash == sha(source)
    rows.append({'source': str(source), 'retained': str(retained),
                 'bytes': retained.stat().st_size, 'sha256': original_hash,
                 'original_mtime_unix': source.stat().st_mtime})
evidence = destination / 'build-evidence'
evidence.mkdir()
for name in ('delivery-06-native-owner-regressions-r1.json',
             'delivery-06-native-owner-regressions-r1.log',
             'delivery-06-native-owner-regressions-r1-cargo-timing.html',
             'delivery-06-native-owner-regressions-r1-junit.xml',
             'delivery-06-native-owner-clippy-r2.log'):
    source = root / 'artifacts/advanced-split/receipts' / name
    if source.is_file():
        shutil.copy2(source, evidence / name)
# Preserve any scratch root instead of letting Cargo discard unknown fixture state.
scratch = target / 'tmp'
if scratch.exists():
    shutil.copytree(scratch, destination / 'target-tmp', symlinks=True)
record = {'retained_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'scope': 'Actual linked harnesses from delivery-06 build, not a fresh build of subsequent source corrections. MCP 07:44 binary is intentionally not labeled current; hybrid capture needs a new build.',
          'source_build_receipt': 'build-evidence/delivery-06-native-owner-regressions-r1.json',
          'binaries': rows, 'free_before_bytes': shutil.disk_usage(root).free,
          'target_to_clean': str(target)}
(destination / 'retention.json').write_text(json.dumps(record, indent=2))
print(json.dumps({'retained': len(rows), 'bytes': sum(row['bytes'] for row in rows),
                  'record': str(destination / 'retention.json')}, indent=2))
