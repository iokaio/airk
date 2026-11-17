# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
from pathlib import Path, PurePosixPath
import zipfile

owner = Path('D:/code/Github/airk/artifacts/tools/vscode-1.145.0-a717-r1')
metadata = json.loads((owner / 'official-metadata.json').read_bytes())
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
archive = owner / 'official.zip'
assert sha(archive) == metadata['sha256hash'] == '79cf4cc5b17357cc64abdd22a2d7d4884d05f07b3f0defbf5fc0d67fe03d9dc4'
runtime = owner / 'runtime'
files = {}
with zipfile.ZipFile(archive) as zipped:
    for row in zipped.infolist():
        path = PurePosixPath(row.filename)
        assert not path.is_absolute() and '..' not in path.parts and '\\' not in row.filename
        if row.is_dir():
            continue
        actual = runtime / row.filename
        assert not actual.is_symlink() and actual.is_file()
        with zipped.open(row) as source:
            digest = hashlib.file_digest(source, 'sha256').hexdigest()
        assert sha(actual) == digest
        files[row.filename] = digest
assert {path.relative_to(runtime).as_posix() for path in runtime.rglob('*') if path.is_file()} == set(files)
app = runtime / metadata['version'][:10] / 'resources/app'
package = json.loads((app / 'package.json').read_text())
product = json.loads((app / 'product.json').read_text())
assert package['version'] == '1.145.0' and product['commit'] == metadata['version']
assert 'target' not in product and not (runtime / 'data').exists()
with (owner / 'stock-file-seal.json').open('x') as output:
    json.dump(files, output, indent=2)
receipt = {'source': 'https://update.code.visualstudio.com/api/versions/1.145.0/win32-x64-archive/stable',
           'archive_sha256': sha(archive), 'archive_bytes': archive.stat().st_size,
           'version': package['version'], 'commit': product['commit'],
           'editor': str(runtime / 'Code.exe'), 'cli': str(app / 'out/cli.js'),
           'editor_sha256': sha(runtime / 'Code.exe'), 'files_verified_against_official_archive': len(files),
           'stock_file_seal_sha256': sha(owner / 'stock-file-seal.json'),
           'distribution': 'unmodified official ZIP; actual versioned layout; no portable data folder',
           'normal_editor_updater_untouched': True,
           'prior_prepare_outcome': 'verified download/extraction; flat app-layout assertion failed; no re-download'}
with (owner / 'receipt.json').open('x') as output:
    json.dump(receipt, output, indent=2)
print(json.dumps(receipt))
