# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
s = Path('artifacts/v190/p9-launch-neo-20261105-r8.py').read_text().replace('-r8', '-r9')
start = s.index('plan.update(')
end = s.index('\nplan_file=', start)
s = s[:start] + '''plan.update(draft=False,pending=[],steps=[
    {'name':'fmt','command':['cargo','fmt','--all','--check']},
    {'name':'linked-control-red','command':['cargo','test','-p','airk-kernel','--test','source_policy','--all-features','--locked','--offline','permanent_trim_rejects_redirected_ignore_control','--','--test-threads=1']}
],not_run=['Intentional pre-fix regression reproduction; failure is expected evidence, not phase acceptance.', 'Complete workspace, runtime, performance, editor, signing, Intel, Linux and publication.'])
''' + s[end:]
Path('artifacts/v190/p9-launch-neo-20261105-r9.py').write_text(s)
print('Prepared R9 launcher; not launched.')
