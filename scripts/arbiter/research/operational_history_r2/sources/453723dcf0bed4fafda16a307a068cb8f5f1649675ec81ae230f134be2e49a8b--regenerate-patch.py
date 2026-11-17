# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib

base = Path(__file__).parent
patch = []
for path in sorted((base / 'source').rglob('*')):
    if not path.is_file():
        continue
    name = path.relative_to(base / 'source').as_posix()
    original = base / 'base' / name
    old = original.read_text(encoding='utf-8') if original.exists() else ''
    new = path.read_text(encoding='utf-8')
    if old == new:
        continue
    patch.extend(difflib.unified_diff(old.splitlines(True), new.splitlines(True),
                 fromfile='a/' + name if original.exists() else '/dev/null', tofile='b/' + name))
(base / 'status-after-f05.patch').write_text(''.join(patch), encoding='utf-8', newline='\n')
