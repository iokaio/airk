# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
s = Path('artifacts/v190/p9-launch-neo-20261105-r8.py').read_text().replace('-r8', '-r10')
start = s.index('plan.update(')
end = s.index('\nplan_file=', start)
s = s[:start] + '''plan.update(draft=False,pending=[],steps=[
    {'name':'fmt','command':['cargo','fmt','--all','--check']},
    {'name':'protected-ignore','command':['cargo','test','-p','airk-ingest','--lib','--all-features','--locked','--offline','policy_ignore','--','--test-threads=1']},
    {'name':'kernel-policy','command':['cargo','test','-p','airk-kernel','--test','source_policy','--all-features','--locked','--offline','--','--test-threads=1']},
    {'name':'policy-runtime','command':['cargo','test','-p','airk-kernel','--lib','--all-features','--locked','--offline','source_policy::tests','--','--test-threads=1']},
    {'name':'watch-policy','command':['cargo','test','-p','airk-kernel','--lib','--all-features','--locked','--offline','source_watch::tests','--','--test-threads=1']},
    {'name':'verification','command':['cargo','test','-p','airk-kernel','--lib','--all-features','--locked','--offline','verification::tests','--','--test-threads=1']},
    {'name':'coverage-related','command':['cargo','test','-p','airk-kernel','--all-features','--locked','--offline','coverage','--','--test-threads=1']},
    {'name':'native-permanent-trim','command':['cargo','test','-p','airk-cli','--test','combined_reads','--all-features','--locked','--offline','permanent_trim','--','--test-threads=1']},
    {'name':'clippy','command':['cargo','clippy','--workspace','--all-targets','--all-features','--locked','--offline','--','-D','warnings']}
],not_run=['Complete Mac workspace; original predecessor-handler evidence remains separately bound to c2039ba6.', 'Fresh Mac runtime, performance, installed editor, signing, notarization, Intel, Linux and publication.'])
''' + s[end:]
start=s.index(",(base/'p9-pre-policy-probe-20261105-r1/source.tar'")
end=s.index(']:\n',start)
s=s[:start]+s[end:]
Path('artifacts/v190/p9-launch-neo-20261105-r10.py').write_text(s)
print('Prepared R10 launcher; not launched.')
