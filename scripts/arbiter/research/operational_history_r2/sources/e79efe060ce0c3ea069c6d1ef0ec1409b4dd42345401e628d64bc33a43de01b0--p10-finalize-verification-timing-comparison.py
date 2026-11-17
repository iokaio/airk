# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib,difflib
root=Path('D:/code/Github/airk'); base=root/'artifacts/v190/p10-worktree'; out=root/'artifacts/v190/p10-verification-timing-comparison-proposed'
baseline=json.loads((out/'baseline.json').read_text(encoding='utf-8'))
chunks=[]
for path,digest in baseline.items():
    raw=(base/path).read_bytes(); assert hashlib.sha256(raw).hexdigest()==digest,path
    chunks.extend(difflib.unified_diff(raw.decode('utf-8').splitlines(True),(out/path).read_text(encoding='utf-8').splitlines(True),fromfile='a/'+path,tofile='b/'+path))
patch=''.join(chunks); (out/'comparison.patch').write_text(patch,encoding='utf-8',newline='\n')
notes={
 'status':'INERT scoped fixture/documentation proposal; no production implementation/budgets changed, no compiler-input edits or Cargo/native reruns. Apply only after complete root workspace failure census and independent review.',
 'patch_sha256':hashlib.sha256(patch.encode()).hexdigest(),
 'source':'Root reports a7458f86/945 runtime-final-r2; baseline.json hashes exact two live inputs.',
 'observed_failure':'combined_reads permanent_trim verified CLI parity: remaining verification limits deadline_ms/effective_deadline_ms1999 vs2000. Actual receipts both verified1source/71bytes, same exact source/hash/profile/publication/scope/references, no limitshits. Full suite was still collecting when proposal prepared.',
 'root_cause':'combined_frontend.rs1213-1224 derives actual member verification deadline with duration.as_millis after beginning2000ms window; submillisecond elapsed scheduling makes1999/2000 legitimate independent observations. Kernel verification.rs339-345 discloses actual received allowance. Existing stable() normalizes outer allocation remainingdeadline but not this nested allowance.',
 'changes':[
  'logical_comparison_copy for known combined-read schema calls existing body(raw) again to authenticate complete ORIGINAL delivery before Core materialization; validates original verification projection reference bindings before changing any comparison field.',
  'After Core strictly materializes complete member results, helper visits ONLY member.result.source_verification with exact known saved-source-verification schema. Typed full receipt parse validates shape without overwriting/reducing fields; typed MemberAllocation requires exact member index and bounds effective<=actualdeadline<=admitted verificationdeadline<=Core maximum, source and byte allowances<=admitted.',
  'Only known per-member source_verification.limits.deadline_ms and effective_deadline_ms become0 in logical comparison copies. Configured/member admitted verification allocation, hit flags, fixed budgets, status, sources, hashes, counts, provenance, quota and unrelated deadline fields remain literal. Raw replies/artifacts remain unchanged.',
  'Docs/read-interface-parity.md precisely records this narrow timing exception and unchanged production budgets.'
 ],
 'caller_audit':{
  'stable':'Only parity_difference invokes stable at root; recursive calls already strip dated/request observations. New helper is outside generic stable recursion to avoid erasing source payload/generic deadline properties.',
  'canonical_comparison':'parity_difference ->logical_comparison_copy ->authenticated strict Core reconstruction ->bounded exact member normalization ->existing stable ->first_difference; unchanged save_replies retains original passed bodies.',
  'callers':'Combined reads actual native/CLI/MCP full-family parity, direct scoped comparison (non-combined unchanged), recommendations/continuations, permanent_trim::policy_parity, and split actual native/CLI/MCP use same assert_parity/parity_difference. No reduced response or transport implementation added.',
  'scope':'Unknown/noncombined/nested receipts are not normalized. If future new canonical location needs timing qualification it remains a visible mismatch until separately validated; no blind recursive schema cleanup.'
 },
 'tests_proposed':'One deterministic fixture test:1999/1998 vs2000/2000 valid remaining windows equal after helper; actual input unchanged. Reject actualdeadline/effectivedeadline overadmission, source/byte overbudget, crossed member index and lowered admitted deadline. Preserve differences in source path/hash, outcome/status, hits, smaller fixed budgets, quota, unrelated genericdeadline and changed admitted verificationbudget. This is helper logic coverage; actual permanent_trim transport rerun remains required.',
 'validation':'Ignored full Rust source copy parsed/formatted only via rustfmt; no compile, test or source edits. Baseline hashes verified before patch generation. No P10 phase acceptance claim; failure original test did not execute later restart/unblock/retirement assertions.',
 'next_validation':'After current root workspace completes: independent root diff review, apply bounded two-file fixture/docs patch, actual failed permanent_trim target plus deterministic comparator test and relevant parity tests using warm production outputs; preserve complete original failure receipt. Do not rerun broad suite speculatively.'
}
(out/'notes.json').write_text(json.dumps(notes,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'files':len(baseline),'patch_sha256':notes['patch_sha256']}))
