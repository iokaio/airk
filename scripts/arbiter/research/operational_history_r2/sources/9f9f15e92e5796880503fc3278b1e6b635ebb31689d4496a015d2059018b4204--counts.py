# SPDX-License-Identifier: Apache-2.0
import json,re,sys
from pathlib import Path
run=Path(sys.argv[1]);counts={}
for name in ['native-unit','store-unit']:
 text=(run/(name+'.log')).read_text();m=re.search(r'Summary \[[^\]]+\] (\d+) tests run: (\d+) passed',text);assert m and int(m[1])>=20 and m[1]==m[2],(name,text[-2000:]);counts[name]=int(m[1])
for name in ['helper-maintenance','helper-abrupt-owner','helper-native-completion','copied-genesis','managed-setup']:
 text=(run/(name+'.log')).read_text();assert 'test result: ok. 1 passed; 0 failed; 0 ignored;' in text,(name,text[-2000:]);counts[name]=1
(run/'counts.json').write_text(json.dumps(counts,indent=2));print(json.dumps(counts))
