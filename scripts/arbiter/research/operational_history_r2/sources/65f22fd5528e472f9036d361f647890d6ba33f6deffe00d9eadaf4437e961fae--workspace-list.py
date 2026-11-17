# SPDX-License-Identifier: Apache-2.0
import hashlib,json,subprocess,sys
from pathlib import Path
BASE=Path(__file__).resolve().parent; ROOT=BASE.parents[1]
cmd=['cargo','nextest','list','--workspace','--all-features','--locked','--timings','--profile','ci','--ignore-default-filter','--message-format','json']
prefix='final-workspace'
if '--exclude-cli' in sys.argv:
 cmd+=['--exclude','airk-cli'];prefix='final-workspace-libraries'
if '--reuse' in sys.argv:
 data=json.loads((BASE/(prefix+'-test-list.json')).read_bytes())
else:
 result=subprocess.run(cmd,cwd=ROOT,stdout=subprocess.PIPE)
 if result.returncode:raise SystemExit(result.returncode)
 data=json.loads(result.stdout); (BASE/(prefix+'-test-list.json')).write_bytes(result.stdout)
selected=[]; ignored=[]
for identity,suite in data['rust-suites'].items():
 for name,test in suite.get('testcases',{}).items():
  row={'suite':identity,'name':name,'filter':test.get('filter-match'),'ignored':test.get('ignored')}
  if test.get('filter-match',{}).get('status')=='matches':selected.append(row)
  else:ignored.append(row)
expected=['two_hundred_current_edits_reach_object_and_database_plateau','compact_measurements_compare_small_reads_from_large_catalog','manual_backlog_larger_than_one_page_advances_without_unprotected_deletion']
retired={'name':'measure_small_and_larger_sparse_catalog_edits','decision':'Accepted R2 deletion in69677a29; uncalled catalog experiment and six-test integration target removed; S1 review section2. No live or renamed equivalent. Not an executed pass.','evidence':'missing-sparse-campaign-review.md'}
found={name:[row for row in selected if row['name'].split('::')[-1]==name] for name in expected}
assert selected and all(len(rows)==1 for rows in found.values()),found
assert not any(retired['name'] in row['name'] for row in selected+ignored)
binaries={}
for identity,suite in data['rust-suites'].items():
 path=Path(suite['binary-path'])
 with path.open('rb') as stream:binaries[identity]={'path':str(path),'sha256':hashlib.file_digest(stream,'sha256').hexdigest()}
receipt={'command':cmd,'reused_saved_listing':'--reuse' in sys.argv,'retired_campaign_disposition':retired,'selected_count':len(selected),'filtered_or_ignored_count':len(ignored),'restored_campaigns':found,'binaries':binaries,'selected':selected,'filtered_or_ignored':ignored}
(BASE/(prefix+'-selection-current.json')).write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps({k:v for k,v in receipt.items() if k not in ('selected','filtered_or_ignored','binaries')}))
