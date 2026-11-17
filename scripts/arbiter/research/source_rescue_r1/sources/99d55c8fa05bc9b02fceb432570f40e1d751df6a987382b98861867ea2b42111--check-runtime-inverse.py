# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json
p=Path(__file__).resolve().parent
row=json.loads((p/'runtime-source-equivalence.json').read_text(encoding='utf-8'))
s=(p/'native-proposal/crates/airk-retrieval/src/arbiter/runtime.rs').read_text(encoding='utf-8').replace('\r\n','\n')
assert hashlib.sha256(s.encode()).hexdigest()==row['new_normalized_sha256']
lines=s.splitlines(keepends=True)
for e in reversed(row['inverse_edits']):
    assert lines[e['new_start']:e['new_end']]==e['new_lines']
    lines[e['new_start']:e['new_end']]=e['old_lines']
assert hashlib.sha256(''.join(lines).encode()).hexdigest()==row['old_normalized_sha256']
assert ''.join(lines)==(p/'base-runtime.rs').read_text(encoding='utf-8').replace('\r\n','\n')
print('Exact runtime inverse verified; no behavioral or compiled claim.')
