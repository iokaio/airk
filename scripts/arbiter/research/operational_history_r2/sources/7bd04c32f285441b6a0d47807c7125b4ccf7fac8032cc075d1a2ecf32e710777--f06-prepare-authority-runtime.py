# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib, json, shutil, tempfile

root = Path.cwd()
artifacts = root / 'artifacts/optimizations'
old = json.loads((artifacts / 'f06-runtime-location.json').read_text())
task = Path(tempfile.mkdtemp(prefix='airk-optimizations-f06-authority-'))
assert not task.resolve().is_relative_to(root.resolve())
assert not task.resolve().is_relative_to(Path('D:/code/Github/bookbuilder').resolve())
for name in ['scripts', 'reports', 'fixtures']:
    (task / name).mkdir()
scripts = []
for item in old['scripts']:
    if item['name'] == 'f06-healthy-status.py':
        continue
    source = Path(old['task']) / 'scripts' / item['name']
    assert hashlib.sha256(source.read_bytes()).hexdigest() == item['sha256']
    destination = task / 'scripts' / item['name']
    shutil.copyfile(source, destination)
    assert hashlib.sha256(destination.read_bytes()).hexdigest() == item['sha256']
    scripts.append(item)
location = {'task': str(task), 'runtime': str(task / 'runtime'), 'scripts': scripts,
            'native_execution': 'not approved or executed; build preparation only',
            'preserved_previous_task': old['task']}
(artifacts / 'f06-authority-runtime-location.json').write_text(json.dumps(location, indent=2) + '\n')
source = (artifacts / 'f06-build-runtime-corrected.py').read_text()
source = source.replace('f06-runtime-location.json', 'f06-authority-runtime-location.json')
source = source.replace('cf3aeac64af289bf763d3823179700c66741083a48f12f6d83030948bee82807', '5c4333f298c502844abb36a8eedf996ed6aa70fbbc8791876c899845024f5466').replace("'files':707", "'files':708")
source = source.replace('f06-runtime-build-corrected', 'f06-authority-runtime-build')
(artifacts / 'f06-build-authority-runtime.py').write_text(source)
print(json.dumps(location, indent=2))
