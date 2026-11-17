# SPDX-License-Identifier: Apache-2.0
"""Prepare only the reviewed A7 contract hunk; leave current-catalog work unstaged."""
import re
import subprocess
from pathlib import Path

result = subprocess.run(['git', '-c', 'safe.directory=D:/code/Github/airk', 'diff', '--',
                         'docs/split/contracts.md'], capture_output=True, check=True)
pieces = re.split(rb'(?m)(?=^@@ )', result.stdout)
assert pieces[0].startswith(b'diff --git a/docs/split/contracts.md ')
hunks = [piece for piece in pieces[1:] if b'**Workbench answer source locator (A7,' in piece]
assert len(hunks) == 1 and b'U95 comparison retirement' not in hunks[0]
Path('artifacts/advanced-split/a7-contract-only.patch').write_bytes(pieces[0] + hunks[0])
