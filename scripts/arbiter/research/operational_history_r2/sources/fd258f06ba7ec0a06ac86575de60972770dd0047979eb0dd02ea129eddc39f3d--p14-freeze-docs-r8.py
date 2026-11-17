# SPDX-License-Identifier: Apache-2.0
import difflib,hashlib,json,re,shutil
from pathlib import Path
B=Path(__file__).resolve().parent
OLD=B/'p14-current-docs-composed-r7'
NEW=B/'p14-current-docs-composed-r8'
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
proof=B/'p14-current-windows-final-r1.json'
assert sha(proof)=='66ad2c7f6cf0ac8b9a227176db3f631d6badc224b683c12c9aed258697628373'
qualification=json.loads(proof.read_text());expected={'sha256':'237f896933ad59a4075f9395fd53fcb92e376946faf1fc01036c139fc0533c0f','files':960}
assert qualification['status']=='passed' and qualification['native_source']==expected
for step in qualification['steps']:
 assert step['exit_code']==0 and sha(step['receipt'])==step['receipt_sha256']
 receipt=json.loads(Path(step['receipt']).read_text())
 assert receipt['exit_code']==receipt['actual_exit_code']==0 and receipt['source_unchanged'] is True
 assert receipt['source_before']==receipt['source_after']==expected
 log=Path(step['receipt'].replace('.receipt.json','.log'));assert sha(log)==receipt['log_sha256']
 for timing in step['timings']:assert sha(timing['path'])==timing['sha256']
 if 'junit' in step:assert sha(step['junit']['path'])==step['junit']['sha256']
body=(B/'p14-current-full-nextest-r1.log').read_text()
assert '2627 tests run: 2627 passed (2 slow), 51 skipped' in body
for name in ['compact_measurements_compare_small_reads_from_large_catalog','two_hundred_current_edits_reach_object_and_database_plateau','measure_small_and_larger_sparse_catalog_edits','manual_backlog_larger_than_one_page_advances_without_unprotected_deletion']:
 assert re.search(r'^\s*PASS .*'+name+r'$',body,re.M),name
docbody=(B/'p14-current-doctests-r1.log').read_text();assert docbody.count('Doc-tests ')==8 and docbody.count('test result: ok. 0 passed; 0 failed;')==8
NEW.mkdir(exist_ok=False)
shutil.copytree(OLD/'base',NEW/'base');shutil.copytree(OLD/'source',NEW/'source')
m=json.loads((OLD/'manifest.json').read_text())
full=('Current-source Windows qualification passed formatting (**6.244 s**), strict workspace Clippy (**24.3543 s**) and full Nextest: **2,627 passed / 0 failed / 51 skipped**, with no retries. All four restored long campaigns passed: compact reads (**246.170 s**), retention plateau (**146.632 s**), sparse catalog edits (**29.658 s**) and manual backlog (**108.021 s**). Nextest ran for **1,542.644 s**; its wrapper took **1,617.6504 s**. Serial doctests passed across **8 crate targets** in **18.6745 s**; there were **0 eligible examples**, not eight executed examples. Immutable JUnit and Cargo timing reports retain this current-source run. These are selected local qualification results; manual acceptance and release promotion remain separate.')
for row in m['files']:
 p=NEW/'source'/row['path'];s=p.read_text()
 s=s.replace('P14 remains in progress: selected Windows editor coverage passed on documented source identities; final current-source checks and primary acceptance are pending.', 'P14 selected local qualification includes Windows native tests and documented editor coverage. Manual acceptance and release promotion remain separate.')
 s=s.replace('Current full-source qualification remains pending. P14 is **not accepted**.',full)
 s=s.replace('current full qualification and primary acceptance remain pending. P14 is not accepted.', 'current Windows native qualification also passed. These selected local results do not establish wider platform or release qualification.')
 s=s.replace('current full qualification and primary acceptance remain pending.', 'current Windows native qualification also passed; manual acceptance and release promotion remain separate.')
 s=s.replace('current full qualification and primary acceptance pending.', 'current Windows native qualification also passed; platform limits remain explicit.')
 s=s.replace('Primary acceptance remains unresolved.', 'Selected local observations retain their documented scope.')
 s=s.replace('Primary review remains pending.', 'Selected local observations retain their documented scope.')
 s=s.replace('primary review remains pending.', 'selected local observations retain their documented scope.')
 s=s.replace('**P14 is not accepted.** Deleted-source recovery', 'Deleted-source recovery')
 s=s.replace('**P14 is not accepted.** No new crate', 'No new crate')
 s=s.replace('**P14 is not accepted.** Earlier D4e receipts', 'Earlier D4e receipts')
 if row['path']=='docs/research/plan-v190.md':
  s=s.replace('P14 is in progress on the 1.8.8\nworking candidate;', 'P14 selected local qualification results are recorded for the 1.8.8\nworking candidate;')
  s=s.replace('Current-source full qualification remains pending; fresh whole usage and legacy\npassed; earlier native/package observations retain their original identity.', 'Current Windows native qualification and fresh whole usage/legacy editor checks\npassed. Manual acceptance, further platform qualification and release promotion\nremain separate; earlier observations retain their original identities.')
 if row['path']=='docs/releases/release-1.8.8.md':
  s=s.replace('P14 remains\n**in progress, not accepted** on source', 'P14 selected local qualification results below use source')
  s=s.replace('The P12 record below likewise retains its original source and limits. P14 is not accepted.', 'The P12 record below likewise retains its original source and limits. P14 had not yet been accepted at that dated checkpoint.')
 if row['path']=='docs/research/implementation-v190.md':
  s=s.replace('**Current checkpoint, 6 November 2026 - P14 pending:**', '**Current checkpoint, 6 November 2026 - P14 selected local qualification results:**')
  a=s.index('| P14 | In progress - not accepted |');z=s.index('\n',a)
  s=s[:a]+'| P14 | Selected local qualification | Current `237f8969...`: formatting, strict workspace Clippy, full Nextest **2,627 passed / 0 failed / 51 skipped**, four restored campaigns, serial doctests, optimized producer and fresh whole usage/legacy passed. Six editor scenarios include four retained earlier-source cases. Corpus deadline failure, intermittent picker failure and platform limits remain disclosed; deleted-source recovery and the final local installer remain required |'+s[z:]
  s=s.replace('## P14 - current qualification in progress, 6 November 2026', '## P14 - selected local qualification results, 6 November 2026')
  s=s.replace('Current full-source suite pending; prior intermittent usage failure retained.', 'Full current Nextest passed **2,627 / 0 failed / 51 skipped**, all four restored campaigns passed, strict workspace Clippy and format passed. Serial doctests covered **8 targets / 0 eligible examples**. Prior intermittent usage failure is retained.')
  s=s.replace('| Phase decision | **P14 not accepted**; primary review and final evidence required. Deleted-source recovery remains a separate unfinished follow-up before the local test installer. |', '| Qualification boundary | Selected local evidence is recorded above. Manual acceptance and wider platform/release qualification remain separate. Deleted-source recovery and the final local test installer remain required follow-ups. |')
  s=s.replace('Full current-source gates and primary P14 acceptance remain pending.', 'The current-source native gates above also passed. The aggregate retains its documented mixed identities and does not establish broader platform or publication qualification.')
  s=s.replace('P14 is **not accepted**; version **1.8.8** is unpublished,', 'These are selected local qualification results; version **1.8.8** is unpublished,')
 p.write_text(s,encoding='utf-8',newline='\n')
# Existing older release notes remain actual historical bytes.
old_details=re.search(b'<details>.*?</details>',(NEW/'base/README.md').read_bytes(),re.S).group()
assert re.search(b'<details>.*?</details>',(NEW/'source/README.md').read_bytes(),re.S).group()==old_details
patch=''
for row in m['files']:
 rel=row['path'];p=NEW/'source'/rel;row['proposed_sha256']=sha(p)
 assert sha(NEW/'base'/rel)==row['base_sha256']
 assert sha(B/'p10-worktree'/rel)==row['base_sha256'],'Live documentation changed before freeze'
 patch+=''.join(difflib.unified_diff((NEW/'base'/rel).read_text().splitlines(True),p.read_text().splitlines(True),fromfile='a/'+rel,tofile='b/'+rel))
(NEW/'current-docs.patch').write_text(patch,encoding='utf-8',newline='\n')
m.update(status='FINAL R8 observed selected-local qualification prose; root application/review/late checks remain separate; no human acceptance or publication asserted',patch_sha256=sha(NEW/'current-docs.patch'),previous_manifest_sha256=sha(OLD/'manifest.json'))
m['new_actual_evidence']['p14-current-windows-final-r1.json']=sha(proof)
m['pending']=['Root primary combined diff/prose review and final late tooling/doc checks','Formal local checkpoint decision recorded separately after required checks','Deleted-source recovery, final local Windows manual-test installer and read-only PR60 review remain mandatory subsequent work','Manual acceptance/publication and unselected platform/resource campaigns remain separate']
m['full_native_result']={'source':expected,'passed':2627,'failed':0,'skipped':51,'restored_campaigns':4,'doctest_targets':8,'eligible_doctest_examples':0,'record_sha256':sha(proof)}
(NEW/'manifest.json').write_text(json.dumps(m,indent=2)+'\n')
print(json.dumps({'patch_sha256':m['patch_sha256'],'manifest_sha256':sha(NEW/'manifest.json'),'files':len(m['files'])}))
