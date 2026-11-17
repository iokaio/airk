# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json
p=Path('artifacts/optimizations/f11-consolidated');r=json.loads((p/'BASE.json').read_text(encoding='utf-8'))
f10=Path('artifacts/optimizations/f10-consolidated')
for name,identity in r['files'].items():
 if identity.get('absent'):
  assert not (f10/'source'/name).exists() and not Path(name).exists(),name
 else:
  actual=f10/'source'/name if identity['layer']=='f10-consolidated/source' else Path(name)
  assert hashlib.sha256(actual.read_bytes()).hexdigest()==identity['sha256'],name
print('effective_f10_plus_live_base_matches',len(r['files']))
