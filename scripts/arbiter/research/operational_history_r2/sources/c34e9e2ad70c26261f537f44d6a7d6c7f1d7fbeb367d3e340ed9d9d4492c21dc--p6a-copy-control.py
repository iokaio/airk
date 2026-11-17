# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil

source = Path('artifacts/v190/p5-188-qualified-runtime')
output = Path('artifacts/v190/p6a-p5-control-copy')
spec = importlib.util.spec_from_file_location('native_package', 'scripts/package-native.py')
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)
build = json.loads((source / 'airk-build.json').read_text())
assert build['source'] == {'sha256': '594438057cae526009b40e960ab443856bcdc6006c41ca9560379ff2175a6510', 'files': 784}
files, _ = package.validate_runtime(source, '1.8.8', 'x86_64-pc-windows-msvc', build['source'])
output.mkdir(exist_ok=False)
for name in files:
    destination = output / name
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source / name, destination)
after, _ = package.validate_runtime(output, '1.8.8', 'x86_64-pc-windows-msvc', build['source'])
assert files == after == package.inventory(source)
receipt = dict(source_build=build, file_count=len(files), exact_inventory_preserved=True,
               source_permissions_changed=False, source_bytes_unchanged=True,
               note='Copied by existing source owner into fresh staging; no ACL changes')
Path('artifacts/v190/p6a-p5-control-copy.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt, indent=2))
