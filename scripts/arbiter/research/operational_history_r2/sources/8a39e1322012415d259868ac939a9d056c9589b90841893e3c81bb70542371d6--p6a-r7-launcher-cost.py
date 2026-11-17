# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
from pathlib import Path
import shutil
import stat
import zipfile

parent = Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh')
runtime = parent / 'source-r2/artifacts/p6a-r7-gui-signed-0363da3003544eb2a3363140e3c20c78/runtime'
name = 'AIRK.app/Contents/MacOS/airk-launcher-image'
source = runtime / name
def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()
expected = '6e781de8fad7cad3115ccbb57e98b20a3f334f791328c633bc7cc3ccc29616c7'
assert sha(source) == expected
archive = parent / 'artifacts/p6a-r7-added-launcher-cost.zip'
with zipfile.ZipFile(archive, 'x', compression=zipfile.ZIP_DEFLATED) as zipped:
    info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
    info.create_system = 3
    info.external_attr = (stat.S_IFREG | 0o755) << 16
    info.compress_type = zipfile.ZIP_DEFLATED
    with source.open('rb') as stream, zipped.open(info, 'w') as target:
        shutil.copyfileobj(stream, target, length=1_048_576)
with zipfile.ZipFile(archive) as zipped:
    info = zipped.getinfo(name)
    result = dict(schema='airk.private-launcher-size/1', source_sha256=expected,
                  raw_bytes=info.file_size, compressed_member_bytes=info.compress_size,
                  one_member_zip_bytes=archive.stat().st_size,
                  removed_role_copy_bytes=3 * (runtime / 'AIRK.app/Contents/MacOS/airk').stat().st_size,
                  scope='Added member with native archive compression settings; not a complete notarized distribution')
assert sha(source) == expected
(parent / 'artifacts/p6a-r7-launcher-cost.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result, indent=2))
