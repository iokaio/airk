# SPDX-License-Identifier: Apache-2.0
import hashlib,json,subprocess
from pathlib import Path
root=Path.cwd();review=json.loads((root/'artifacts/v190/p9-final-review-r4.json').read_text(encoding='utf-8'))
base=['git','-c','safe.directory='+root.as_posix()]
def run(*args):return subprocess.check_output([*base,*args],cwd=root,text=True).strip()
assert run('config','user.name')=='Tyler Jensen' and run('config','user.email')=='tyler@tsjensen.com'
assert not run('diff','--cached','--name-only'),'Existing staged changes require preservation'
for name,digest in review['files'].items():
 p=root/name;assert p.is_file() and not p.is_symlink() and hashlib.sha256(p.read_bytes()).hexdigest()==digest,name
names=list(review['files']);assert all(not n.startswith(('temp/','artifacts/','.airk/','.codex/')) for n in names)
subprocess.run([*base,'add','--',*names],cwd=root,check=True)
assert set(run('diff','--cached','--name-only').splitlines())==set(names)
subprocess.run([*base,'diff','--cached','--check'],cwd=root,check=True)
message=root/'artifacts/v190/p9-commit-message.txt'
message.write_text('P9: enforce reviewed permanent source exclusions\n\nBind native admission and every evidence delivery to persistent index policy.\nKeep removal publications, physical cleanup, retirement and exact retry receipts\nseparate; preserve source files, index isolation and complete frontend results.\n\nValidate with Windows workspace/runtime gates and focused Mac ARM regressions.\nRecord exact source identities and qualification limits in the phase ledger.\n',encoding='utf-8')
subprocess.run([*base,'commit','-s','--file',str(message)],cwd=root,check=True)
body=run('log','-1','--format=%B');assert 'Signed-off-by: Tyler Jensen <tyler@tsjensen.com>' in body
print('COMMIT',run('rev-parse','HEAD'));print(body)
