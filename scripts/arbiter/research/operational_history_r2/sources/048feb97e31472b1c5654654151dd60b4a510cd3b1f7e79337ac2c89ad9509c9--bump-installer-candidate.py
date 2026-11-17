# SPDX-License-Identifier: Apache-2.0
import importlib.util
from pathlib import Path
root=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('release_version',root/'scripts/release-version.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
for name,value in module.changes('1.8.22','1.8.23',root).items():
 (root/name).write_text(value,encoding='utf-8')
print('Synchronized native/extension manifests and lockfiles: 1.8.22 -> 1.8.23')
