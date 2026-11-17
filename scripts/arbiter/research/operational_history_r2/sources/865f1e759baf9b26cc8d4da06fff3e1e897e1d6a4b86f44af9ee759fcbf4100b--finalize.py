# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,difflib
p=Path(__file__).resolve().parent
repo=p.parents[2]
parent=p.parent/'f10b-current-f11'
name='crates/airk-cli/tests/native_owner_recovery.rs'
assert not (repo/name).exists(),name
sha=lambda path:hashlib.sha256(path.read_bytes()).hexdigest()
expected={'f10b-on-current-f11.patch':'da6602a4bf6864319ff37b4f760e294493b18d3ffed3ea123b5538f5d34aa2bb','BASE.json':'5dbbdcac775c627fef23e830d2486f895527ec327360451cb0452bad9021da16','SOURCE.json':'42e1fc1a92b2647d441866396785733970e398de25ba44d38f2d26ea50ce8353'}
for file,digest in expected.items():assert sha(parent/file)==digest,file
base={'schema':1,'kind':'new manual-owner actual-process integration fixture against frozen F10b current-F11','parent':{'path':str(parent.relative_to(repo)).replace('\\','/'),'files':expected},'files':{name:{'absent':True}},'live_dependency':{'crates/airk-cli/Cargo.toml':sha(repo/'crates/airk-cli/Cargo.toml'),'crates/airk-cli/tests/integrity_restart.rs':sha(repo/'crates/airk-cli/tests/integrity_restart.rs')}}
(p/'BASE.json').write_text(json.dumps(base,indent=2)+'\n',encoding='utf-8',newline='\n')
source=(p/'source'/name).read_text(encoding='utf-8')
(p/'f10b-process-fixtures.patch').write_text(''.join(difflib.unified_diff([],source.splitlines(True),fromfile='/dev/null',tofile='b/'+name)),encoding='utf-8',newline='\n')
(p/'SOURCE.json').write_text(json.dumps({'schema':1,'files':{name:sha(p/'source'/name)}},indent=2)+'\n',encoding='utf-8',newline='\n')
print('new fixture source lines',len(source.splitlines()))
