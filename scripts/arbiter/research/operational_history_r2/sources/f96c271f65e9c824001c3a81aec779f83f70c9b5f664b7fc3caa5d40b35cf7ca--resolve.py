# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import re,json,subprocess
p=Path(__file__).resolve().parent
name='crates/airk-store/src/service_collection_tests.rs'
texts=[(p/'merge-inputs'/name).with_suffix('.rs.'+s).read_text(encoding='utf-8') for s in ['live','base','proposal']]
pat=re.compile(r'(?m)^(?:#\[[^\n]*\]\n)*fn (\w+)[\s\S]*?^}\n')
f=[{m.group(1):m for m in pat.finditer(t)} for t in texts]
result=texts[0]
changes=[]
for n,old in f[1].items():
    if n not in f[2] or old.group()==f[2][n].group(): continue
    current=f[0][n]
    # Seed-only F10b modifications; preserve live owning-registry reopen correction.
    proposed=f[2][n].group()
    if n=='installed_data_root_rename_preserves_publication_settings_pause_and_receipts':
        proposed=proposed.replace('ServiceRegistry::open_saved(&control)?','ServiceRegistry::open(&control, namespace)?')
    changes.append((current.start(),current.end(),proposed))
for start,end,value in sorted(changes,reverse=True): result=result[:start]+value+result[end:]
new=[m.start() for n,m in f[2].items() if n not in f[1]]
assert new
result=result.rstrip()+'\n\n'+texts[2][min(new):]
assert all(n in {m.group(1) for m in pat.finditer(result)} for n in f[0])
(p/'source'/name).write_text(result,encoding='utf-8',newline='\n')
# Keep new existing-only exclusion plus live public lock documentation.
name='crates/airk-store/src/lib.rs'
path=p/'source'/name
text=path.read_text(encoding='utf-8')
conflict=re.compile(r'(?m)^<<<<<<<[^\n]*\n([\s\S]*?)^=======\n([\s\S]*?)^>>>>>>>[^\n]*\n')
assert len(list(conflict.finditer(text)))==1
text=conflict.sub(lambda m:m.group(2)+m.group(1),text)
path.write_text(text,encoding='utf-8',newline='\n')
# F11 removed future-only accessors; F10b now supplies the actual owning callers.
name='crates/airk-store/src/service_data_scope.rs'
path=p/'source'/name
text=path.read_text(encoding='utf-8')
assert 'fn native_integrity_revoked' not in text
insert='''    pub(crate) fn native_integrity_revoked(&self) -> bool {
        self.1.revoked()
    }
    pub(crate) fn native_control_root(&self) -> &Path {
        &self.0.control
    }
'''
assert text.count('impl BoundDataRoot {')==1
text=text.replace('impl BoundDataRoot {','impl BoundDataRoot {\n'+insert)
path.write_text(text,encoding='utf-8',newline='\n')
print('resolved2files; reinstated2actual-caller accessors')
