# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/v190/p9-launch-neo-20261105.py')
s=p.read_text()
for label in ['p9-source-20261105','p9-native-20261105','p9-neo-plan-20261105','p9-neo-owner-20261105','p9-neo-active-20261105']:
    s=s.replace(label+'-r1',label+'-r3')
s=s.replace('/opt/homebrew/bin/python3.13','/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools/python3.13')
a=s.index("    {'name':'workspace-tests'");b=s.index("],not_run=",a)
s=s[:a]+'''    {'name':'kernel-policy-unit','command':['cargo','test','-p','airk-kernel','--lib','--all-features','--locked','--offline','source_policy','--','--test-threads=1']},
    {'name':'kernel-policy-native','command':['cargo','test','-p','airk-kernel','--test','source_policy','--all-features','--locked','--offline','--','--test-threads=1']},
    {'name':'kernel-fts','command':['cargo','test','-p','airk-kernel','--test','current_fts','--all-features','--locked','--offline','--','--test-threads=1']},
    {'name':'kernel-watch','command':['cargo','test','-p','airk-kernel','--lib','--all-features','--locked','--offline','source_watch','--','--test-threads=1']},
    {'name':'store-catalog','command':['cargo','test','-p','airk-store','--lib','--all-features','--locked','--offline','service::registry::catalog::tests','--','--test-threads=1']},
    {'name':'native-permanent-trim','command':['cargo','test','-p','airk-cli','--test','combined_reads','--all-features','--locked','--offline','permanent_trim','--','--test-threads=1']},
    {'name':'clippy','command':['cargo','clippy','--workspace','--all-targets','--all-features','--locked','--offline','--','-D','warnings']}
'''+s[b:]
s=s.replace("not_run=['Installed", "not_run=['Full Mac workspace rerun after these corrections; the R2 failures remain separate evidence.','Fresh Mac runtime build and smoke after corrections.','Installed")
Path('artifacts/v190/p9-launch-neo-20261105-r3.py').write_text(s)
