# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json
p=Path(__file__).resolve().parent
r=json.loads((p/'runtime-source-equivalence.json').read_text(encoding='utf-8'))
s=(p/'native-proposal/crates/airk-retrieval/src/arbiter/runtime.rs').read_text(encoding='utf-8').replace('\r\n','\n')
assert hashlib.sha256(s.encode()).hexdigest()==r['new_normalized_sha256']
lines=s.splitlines(True)
for e in reversed(r['inverse_edits']):
 assert lines[e['new_start']:e['new_end']]==e['new_lines']
 lines[e['new_start']:e['new_end']]=e['old_lines']
assert hashlib.sha256(''.join(lines).encode()).hexdigest()==r['old_normalized_sha256']
assert ''.join(lines)==(p/'base-runtime.rs').read_text(encoding='utf-8').replace('\r\n','\n')
print('Exact final-graph runtime inverse verified; no behavioral claim.')
