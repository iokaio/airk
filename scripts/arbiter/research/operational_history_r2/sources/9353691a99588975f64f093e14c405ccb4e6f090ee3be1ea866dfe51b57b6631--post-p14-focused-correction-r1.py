# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,os,re,subprocess,sys
B=Path(__file__).resolve().parent;W=B/'p10-worktree'
catalog=json.loads((B/'post-p14-first-pass-qualification-r6/command-catalog.json').read_text())
old=next(s['command'] for s in catalog['windows_steps'] if s['name']=='focused-list')
expression=old[old.index('-E')+1]
extra=[('airk-store','metadata_only_registration_retires_without_claiming_uninitialized_storage'),('airk-store','uninitialized_retirement_proof_refuses_native_state_and_replaced_root'),('airk-store','metadata_only_shared_root_retirement_never_disposes_credit_without_advisory'),('airk-cli','actual_registry_scope_metadata_tracks_removed_reregistered_ambiguity_and_namespace'),('airk-cli','root_scope_survives_replaced_storage_and_refuses_ambiguous_indexes'),('airk-store','idempotent_active_registration_clearing_same_root_tombstone_advances_scope_revision'),('airk-store','active_registration_clearing_tombstone_must_advance_registry_revision'),('airk-store','completed_removal_never_reuses_an_unadmitted_registration_epoch'),('airk-store','retire_missing_source_cancels_work_preserves_other_projects_and_fences_stale_reports'),('airk-cli','cli_reference_documents_every_public_command')]
expression+=' | '+' | '.join('(package('+p+') & test('+n+'))' for p,n in extra)
wanted=re.findall(r'package\(([^)]+)\) & test\(([^)]+)\)',expression)
assert len(wanted)==47
common=['--locked','--offline','--timings','-p','airk-core','-p','airk-store','-p','airk-cli','--lib','--test','combined_reads','--bin','airk','--all-features','--profile','ci','-E',expression]
env=os.environ.copy();env['PATH']=str(B.parent/'tools/nextest')+os.pathsep+env['PATH']
for key in ['AIRK_CATALOG_CANDIDATE_BINARY','AIRK_TEST_BINARY','AIRK_P10_RESOURCE_OUTPUT_DIR','AIRK_P11B_RESOURCE_OUTPUT_DIR','AIRK_WRITE_CLI_REFERENCE']:env.pop(key,None)
listed=B/'post-p14-focused-correction-list-r1.json'
with listed.open('xb') as f:r=subprocess.run(['cargo','nextest','list',*common,'--message-format','json'],cwd=W,env=env,stdout=f)
if r.returncode:sys.exit(r.returncode)
v=json.loads(listed.read_text());cases=[{'package':s['package-name'],'name':n,'ignored':t['ignored']} for s in v['rust-suites'].values() for n,t in s['testcases'].items() if t['filter-match']['status']=='matches']
assert len(cases)==47 and all(not c['ignored'] for c in cases)
assert all(sum(c['package']==p and (c['name']==n or c['name'].endswith('::'+n)) for c in cases)==1 for p,n in wanted)
with (B/'post-p14-focused-correction-inventory-r1.json').open('x') as f:json.dump({'expected':47,'new':34,'related':13,'cases':cases},f,indent=2)
print(json.dumps({'matched':47,'new':34,'related':13}),flush=True)
sys.exit(subprocess.run(['cargo','nextest','run',*common,'--no-fail-fast','--retries','0','--test-threads','4'],cwd=W,env=env).returncode)
