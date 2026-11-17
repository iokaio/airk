# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
from pathlib import Path, PurePosixPath
import shutil
import stat
import urllib.request
import zipfile

owner = Path('D:/code/Github/airk/artifacts/tools/vscode-1.145.0-a717-r1')
assert not owner.exists()
assert shutil.disk_usage(owner.parent).free > 3 * 1024**3
url = 'https://update.code.visualstudio.com/api/versions/1.145.0/win32-x64-archive/stable'
with urllib.request.urlopen(url, timeout=30) as response:
    raw = response.read(65537)
assert len(raw) <= 65536
metadata = json.loads(raw)
assert metadata['version'] == 'a717861c07586a53d83b80d742cd3f59b20a6273'
assert metadata['sha256hash'] == '79cf4cc5b17357cc64abdd22a2d7d4884d05f07b3f0defbf5fc0d67fe03d9dc4'
assert metadata['url'].startswith('https://vscode.download.prss.microsoft.com/')
owner.mkdir()
(owner / 'official-metadata.json').write_bytes(raw)
archive = owner / 'official.zip'
total = 0
digest = hashlib.sha256()
with urllib.request.urlopen(metadata['url'], timeout=120) as response, archive.open('xb') as output:
    while chunk := response.read(1024 * 1024):
        total += len(chunk)
        assert total <= 512 * 1024**2
        output.write(chunk)
        digest.update(chunk)
assert digest.hexdigest() == metadata['sha256hash']
runtime = owner / 'runtime'
runtime.mkdir()
with zipfile.ZipFile(archive) as zipped:
    entries = zipped.infolist()
    assert len(entries) <= 50000 and sum(row.file_size for row in entries) < 2 * 1024**3
    for row in entries:
        path = PurePosixPath(row.filename)
        assert not path.is_absolute() and '..' not in path.parts and '\\' not in row.filename
        assert not stat.S_ISLNK(row.external_attr >> 16)
    zipped.extractall(runtime)
app = runtime / 'resources/app'
package = json.loads((app / 'package.json').read_text())
product = json.loads((app / 'product.json').read_text())
assert package['version'] == '1.145.0' and product['commit'] == metadata['version']
assert 'target' not in product, 'Stock ZIP must preserve its actual distribution identity'
assert not (runtime / 'data').exists(), 'Canonical test profiles must remain explicitly isolated'
receipt = {'source': url, 'archive_sha256': digest.hexdigest(), 'archive_bytes': total,
           'version': package['version'], 'commit': product['commit'],
           'editor': str(runtime / 'Code.exe'), 'cli': str(app / 'out/cli.js'),
           'editor_sha256': hashlib.sha256((runtime / 'Code.exe').read_bytes()).hexdigest(),
           'distribution': 'unmodified official ZIP; no portable data folder or normal-install change',
           'normal_editor_updater_untouched': True}
with (owner / 'receipt.json').open('x') as output:
    json.dump(receipt, output, indent=2)
print(json.dumps(receipt))
