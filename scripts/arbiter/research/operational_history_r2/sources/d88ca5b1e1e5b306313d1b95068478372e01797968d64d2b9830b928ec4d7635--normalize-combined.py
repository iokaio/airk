# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib, re

directory = Path(__file__).parent
lines = (directory / 'f09-combined.patch').read_text().splitlines()
output = []
position = 0
while position < len(lines):
    assert lines[position].startswith('--- a/')
    old_name, new_name = lines[position][4:], lines[position + 1][4:]
    header = re.fullmatch(r'@@ -1,(\d+) \+1,(\d+) @@', lines[position + 2])
    assert header
    position += 3
    before, after = [], []
    while position < len(lines) and not lines[position].startswith('--- a/'):
        line = lines[position]
        if line.startswith((' ', '-')):
            before.append(line[1:] + '\n')
        if line.startswith((' ', '+')):
            after.append(line[1:] + '\n')
        position += 1
    assert len(before) == int(header[1]) and len(after) == int(header[2])
    output.extend(difflib.unified_diff(before, after, fromfile=old_name, tofile=new_name, n=3))
(directory / 'f09-combined-minimal.patch').write_text(''.join(output), newline='\n')
