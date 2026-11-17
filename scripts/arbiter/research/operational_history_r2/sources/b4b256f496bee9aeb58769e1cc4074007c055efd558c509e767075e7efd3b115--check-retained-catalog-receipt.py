# SPDX-License-Identifier: Apache-2.0
"""Read one existing fixture receipt in memory; persist diagnostics, never its payload."""
import hashlib
import json
from pathlib import Path
import sqlite3
import subprocess
import time

base = Path(__file__).resolve().parent
root = base.parents[2]
database = root / 'artifacts/editor-qualification/native-installed-Ujos45/multi/native-service/control/service-1.4.sqlite3'
helper = base / 'check-retained-catalog-receipt.cjs'
def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()
assert not any(Path(str(database) + suffix).exists() and Path(str(database) + suffix).stat().st_size for suffix in ('-wal', '-journal'))
before = (database.stat().st_size, database.stat().st_mtime_ns, digest(database))
compiled = {p.relative_to(root).as_posix():digest(p) for p in (root/'packages/vscode/out').rglob('*') if p.is_file()}
started = time.monotonic()
with sqlite3.connect(database.as_uri() + '?mode=ro&immutable=1', uri=True) as connection:
    rows = connection.execute("SELECT payload FROM index_operation_receipts WHERE id=? AND kind=? AND stage=? LIMIT 2", ('d809342f6fba93cd86503ca83de45938', 'catalog_management_fenced', 'metadata_applied')).fetchall()
    assert len(rows) == 1
    payload = rows[0][0]
    payload = payload.encode('utf-8') if isinstance(payload, str) else bytes(payload)
    assert 0 < len(payload) <= 65536
    payload_sha = hashlib.sha256(payload).hexdigest()
    assert payload_sha == '090b2ef4db7ecac7a71273f479ce543d6a2f0e7a10988e7a5e81d69aafd189c7'
    result = subprocess.run(['node', str(helper)], input=payload, capture_output=True, cwd=root)
del payload, rows
after = (database.stat().st_size, database.stat().st_mtime_ns, digest(database))
out = base/'clean-break-r38-retained-receipt-t0-r1.log'
with out.open('xb') as stream:
    stream.write(result.stdout)
    stream.write(result.stderr)
receipt = dict(exit_code=result.returncode, elapsed_seconds=time.monotonic()-started,
    helper_sha256=digest(helper), database_unchanged=before==after,
    database_sha256=before[2], receipt_sha256=payload_sha,
    compiled_inputs_unchanged=compiled=={p.relative_to(root).as_posix():digest(p) for p in (root/'packages/vscode/out').rglob('*') if p.is_file()},
    compiled_input_count=len(compiled), payload_saved=False,
    envelope='canonical handler envelope reconstructed from retained durable record; not a captured wire frame')
with (base/'clean-break-r38-retained-receipt-t0-r1.json').open('x') as stream:
    json.dump(receipt, stream, indent=2)
print(json.dumps(receipt))
assert before == after and receipt['compiled_inputs_unchanged']
raise SystemExit(result.returncode)
