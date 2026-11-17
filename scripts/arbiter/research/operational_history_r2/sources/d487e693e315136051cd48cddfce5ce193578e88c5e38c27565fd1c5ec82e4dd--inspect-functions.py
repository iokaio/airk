# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import re,json,subprocess
p=Path('artifacts/optimizations/f10b-current-f11')
name='crates/airk-store/src/service_collection_tests.rs'
texts=[(p/'merge-inputs'/name).with_suffix('.rs.'+s).read_text(encoding='utf-8') for s in ['live','base','proposal']]
pat=re.compile(r'(?m)^(?:#\[[^\n]*\]\n)*fn (\w+)[\s\S]*?^}\n')
functions=[{m.group(1):m for m in pat.finditer(t)} for t in texts]
print('functions',list(map(len,functions)))
print('added',list(functions[2].keys()-functions[1].keys()))
print('changed',[n for n in functions[1] if n in functions[2] and functions[1][n].group()!=functions[2][n].group()])
