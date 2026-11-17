# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib,difflib
root=Path('D:/code/Github/airk');base=root/'artifacts/v190/p10-worktree';out=root/'artifacts/v190/p10-full-scope-policy-metadata-proposed'
baseline=json.loads((out/'baseline.json').read_text(encoding='utf-8'));chunks=[]
for path,digest in baseline.items():
    raw=(base/path).read_bytes();assert hashlib.sha256(raw).hexdigest()==digest,path
    chunks.extend(difflib.unified_diff(raw.decode('utf-8').splitlines(True),(out/path).read_text(encoding='utf-8').splitlines(True),fromfile='a/'+path,tofile='b/'+path))
patch=''.join(chunks);(out/'metadata.patch').write_text(patch,encoding='utf-8',newline='\n')
notes={
'status':'INERT proposed owning correction for actual P10 failure. No tracked source/compiler-input edits, Cargo/native execution/SSH. Root independently reviews/applies and owns compiler.',
'patch_sha256':hashlib.sha256(patch.encode()).hexdigest(),'files':list(baseline),
'failure_evidence':'Root p10-trim-status-diagnostic-r1.log: actual whole-root Project2 status returns typed scoped_read_unavailable, not timeout or dropped field; actual2000ms budget with1917ms remaining. All earlier allowed/verified/blocked paths succeeded. Source9d85c67c/945 diagnostic only; preserved runtime paira745.',
'cause':'Kernel metadata admission used policy-inclusive CurrentRead::restricts_paths. Any permanent deny sets policy.restricts=true, even though scoped grant covers every published source and policy was actually applied/attested. That conflates source-content filtering with Project metadata authority.',
'minimal_production_changes':[
'Store readonly selected_scope_restricts_paths exposes ONLY existing immutable attached Project coverage flag (computed from this exact published root/aliases). Existing restricts_paths remains identical union of permanent policy restriction OR selected-scope restriction; no new authorization setters or cache dependence.',
'Kernel SessionScope::require_index_metadata retains scope fingerprint, read.token/live fence and configured/published roots agreement, using only selected-scope restriction for whole-index metadata authority. Existing per-read source-policy ready/digest/epoch/publication/revocation checks remain unchanged.',
'Three optional-readiness owners current/runtime.rs,current/search.rs,references.rs use selected Project coverage for metadata disclosure. Permanent filtering still applies to all actual paths/edges/candidates. Full unscoped/direct native policy reads can also disclose policy-qualified published readiness.',
'Current search empty-filter optimization remains policy-inclusive restricts_paths. Store accepts_path/require_path/endpoint/reference filtering, source-policy predicates, read lifetimes and all source capture remain unchanged.'
],
'meaningful_actual_regression':'New Kernel unit full_project_metadata_and_readiness_survive_actual_permanent_deny initializes an actual service collection, indexes allowed+private Rust files, applies native SourcePolicy removal via existing Kernel owner (not forged metadata), freezes matching actual policy epoch/digest in full Project authority, verifies real Store union restriction stilltrue but selected restrictionfalse; actual denied path refuses; summary/status/scoped extraction-policy metadata/roots/readiness succeed, counts1 and descriptor preserved. Subset public path keeps metadata refusal/readiness omission, old policy epoch/digest refuses, revoked grant refuses, original source files unchanged. Physical cleanup completion is not asserted because external selections may legitimately pin old objects.',
'existing_negative_and_endtoend':'Existing current_metadata_handlers_accept_full_source_and_refuse_subset_or_new_alias remains unchanged and supplies new/ungranted alias negative after real publication. Existing exact actual permanent_trim test covers native/CLI/MCP all-read-family parity, restart/unblock/retirement suffix and source preservation; do not relax assertions. New deny test adds partial Project with active policy. No new fixtures or typed control authority bypass.',
'validation':'All five ignored full Rust copies parsed/formatted with rustfmt --edition2024 --config skip_children=true; baseline SHA256 checked. No typecheck/test/native execution. Apply-check to be collected separately. Root must run focused new Kernel test + existing metadata negatives + exact failed native transport test, then assess complete combined diff and remaining gates.',
'coordination':'Existing three actual fixture corrections and P11b immutable/native proposal artifacts untouched. P11a/P11b proposal baseline hashes must rebase against accepted P10 correction; this patch carries no profile3/OCR/model changes.'
}
(out/'notes.json').write_text(json.dumps(notes,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'files':len(baseline),'patch_sha256':notes['patch_sha256']}))
