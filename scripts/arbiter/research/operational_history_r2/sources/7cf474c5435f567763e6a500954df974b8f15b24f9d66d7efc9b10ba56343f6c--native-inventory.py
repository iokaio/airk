# SPDX-License-Identifier: Apache-2.0
import json, subprocess, sys
from pathlib import Path
out=Path(__file__).resolve().parent/'windows-r1/native-inventory-r1.json'
selection='(package(airk-cli) & (test(source_worker::) | test(source_supervisor::) | test(catalog_control::) | test(catalog_coordinator::) | test(service_management::receipt_tests) | test(embedding_pool_bootstrap::))) | (package(airk-kernel) & test(source_watch::)) | (package(airk-store) & (test(retirement_evidence) | test(quota::funding::) | test(quota::retired::) | test(real_claimed_cancellation_disposes_paid_shell_then_publishes_and_acknowledges_fixed_slots))) | (package(airk-core) & test(combined_reads::)) | (package(airk-mcp) & test(combined::tests::))'
command=['cargo','nextest','list','--workspace','--lib','--all-features','--locked','--offline','--timings','--ignore-default-filter','-E',selection,'--message-format','json']
with out.open('x',encoding='utf-8') as stream:
    code=subprocess.run(command,stdout=stream,stderr=sys.stdout).returncode
if code:sys.exit(code)
inventory=json.loads(out.read_text(encoding='utf-8'))
selected=[]
for suite in inventory['rust-suites'].values():
    for name,test in suite['testcases'].items():
        if test['filter-match']['status']=='matches':selected.append({'package':suite['package-name'],'name':name,'ignored':test['ignored']})
packages={case['package'] for case in selected}
assert packages=={'airk-cli','airk-core','airk-kernel','airk-store','airk-mcp'},packages
assert all(not case['ignored'] for case in selected)
value={'selection':selection,'command':command,'selected':selected,'counts':{package:sum(case['package']==package for case in selected) for package in sorted(packages)}}
with out.with_name('native-selection-r1.json').open('x',encoding='utf-8') as stream:json.dump(value,stream,indent=2)
print(json.dumps({'selected':len(selected),'counts':value['counts']}))
