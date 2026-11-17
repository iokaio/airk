# SPDX-License-Identifier: Apache-2.0
import json,os,shutil,subprocess,sys,time,xml.etree.ElementTree as ET
from pathlib import Path
base=Path(__file__).resolve().parent;root=base.parents[1];out=base/'windows-r1'
environment=os.environ.copy()
for name in ['AIRK_CATALOG_CANDIDATE_BINARY','AIRK_TEST_BINARY','AIRK_P10_RESOURCE_OUTPUT_DIR','AIRK_P11B_RESOURCE_OUTPUT_DIR']:environment.pop(name,None)
common=['--workspace','--all-features','--locked','--offline','--timings','--ignore-default-filter']
command=['cargo','nextest','list',*common,'--message-format','json']
print(json.dumps({'stage':'inventory','command':command}),flush=True)
inventory_path=out/'full-native-inventory-r1.json'
with inventory_path.open('x',encoding='utf-8') as stream:code=subprocess.run(command,cwd=root,env=environment,stdout=stream,stderr=sys.stdout).returncode
if code:sys.exit(code)
inventory=json.loads(inventory_path.read_text(encoding='utf-8'));selected=[]
for suite in inventory['rust-suites'].values():
    for name,test in suite['testcases'].items():
        if test['filter-match']['status']=='matches' and not test['ignored']:selected.append((suite['package-name'],name))
campaigns=['two_hundred_current_edits_reach_object_and_database_plateau','compact_measurements_compare_small_reads_from_large_catalog','manual_backlog_larger_than_one_page_advances_without_unprotected_deletion','measure_small_and_larger_sparse_catalog_edits']
for name in campaigns:assert sum(case.rsplit('::',1)[-1]==name for _,case in selected)==1,name
assert len(selected)>2661,len(selected)
print(json.dumps({'selected':len(selected),'restored_campaigns':campaigns}),flush=True)
command=['cargo','nextest','run',*common,'--profile','ci','--no-fail-fast','--retries','0','--test-threads','4']
print(json.dumps({'stage':'full_suite','command':command}),flush=True)
started=time.monotonic();code=subprocess.run(command,cwd=root,env=environment).returncode
report=root/'target/nextest/ci/junit.xml';saved=out/'full-native-r1.junit.xml';assert not saved.exists()
if report.exists():shutil.copyfile(report,saved)
cases=list(ET.parse(saved).iter('testcase')) if saved.exists() else []
failed=[case.get('name') for case in cases if case.find('failure') is not None or case.find('error') is not None]
skipped=[case.get('name') for case in cases if case.find('skipped') is not None]
passed=[case.get('name') for case in cases if case.find('failure') is None and case.find('error') is None and case.find('skipped') is None]
summary={'exit_code':code,'selected':len(selected),'junit_cases':len(cases),'passed':len(passed),'failed':failed,'skipped':skipped,'seconds':time.monotonic()-started,'command':command,'junit':str(saved)}
with (out/'full-native-r1.summary.json').open('x',encoding='utf-8') as stream:json.dump(summary,stream,indent=2)
print(json.dumps(summary),flush=True)
if code:sys.exit(code)
assert not failed and len(passed)==len(selected),(len(passed),len(selected))
for name in campaigns:assert sum(case.rsplit('::',1)[-1]==name for case in passed)==1,name
