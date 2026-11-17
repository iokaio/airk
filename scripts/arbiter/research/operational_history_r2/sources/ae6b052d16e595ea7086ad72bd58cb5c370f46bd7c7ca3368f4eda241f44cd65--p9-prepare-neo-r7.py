# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
s = Path('artifacts/v190/p9-launch-neo-20261105-r6.py').read_text().replace('-r6', '-r7')
start = s.index('plan.update(')
end = s.index('\nplan_file=', start)
s = s[:start] + '''plan.update(draft=False,pending=[],steps=[
    {'name':'fmt','command':['cargo','fmt','--all','--check']},
    {'name':'policy-observation','command':['cargo','test','-p','airk-kernel','--lib','--all-features','--locked','--offline','unblocked_empty_policy_observation','--','--test-threads=1']},
    {'name':'actual-unblock-transports','command':['cargo','test','-p','airk-cli','--test','combined_reads','--all-features','--locked','--offline','permanent_trim_fences_retained_grants_and_preserves_native_cli_mcp_results','--','--test-threads=1']},
    {'name':'clippy','command':['cargo','clippy','--workspace','--all-targets','--all-features','--locked','--offline','--','-D','warnings']}
],not_run=['Complete Mac workspace; R6 focused lifecycle, quota and predecessor evidence remains separately bound to c2039ba6.', 'Fresh Mac runtime, performance, installed editor, signing, notarization, Intel, Linux and publication.'])
''' + s[end:]
Path('artifacts/v190/p9-launch-neo-20261105-r7.py').write_text(s)
print('Prepared R7 launcher; not launched.')
