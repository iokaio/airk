# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib,difflib
root=Path('D:/code/Github/airk'); base=root/'artifacts/v190/p10-worktree'
out=root/'artifacts/v190/p10-verification-timing-comparison-proposed'; out.mkdir(exist_ok=True)
paths=['crates/airk-cli/tests/combined_reads.rs','docs/read-interface-parity.md']
original={p:(base/p).read_text(encoding='utf-8') for p in paths}; source=dict(original)
path=paths[0]
old='''    // body() checked the complete RAW delivery digest first. Core now checks'''
new='''    // Reauthenticate even when a caller already peeled its transport envelope.
    // Never normalize a receipt or a dictionary before checking the actual wire.
    body(raw)?;
    airk_core::verification::validate_projection(raw)?;
    // body() checked the complete RAW delivery digest first. Core now checks'''
assert source[path].count(old)==1; source[path]=source[path].replace(old,new)
old='''        .context("strictly reconstruct complete combined logical view")?;
    if let Some(dictionary)'''
new='''        .context("strictly reconstruct complete combined logical view")?;
    normalize_member_verification_timing(&mut logical)?;
    if let Some(dictionary)'''
assert source[path].count(old)==1; source[path]=source[path].replace(old,new)
anchor='''async fn native_read('''
helper='''/// Only these per-member receipt fields contain an elapsed allowance, not
/// the configured shared verification budget. Every other field remains literal.
fn normalize_member_verification_timing(logical: &mut Value) -> Result<()> {
    if logical["schema"] != "airk.combined-read/1" { return Ok(()); }
    let members = logical["members"].as_array_mut().context("missing comparison members")?;
    ensure!(members.len() <= airk_core::index_catalog::MAX_SELECTION,
        "comparison member bound exceeded");
    for member in members {
        let Some(receipt) = member.get("result").and_then(|result| result.get("source_verification"))
        else { continue; };
        if receipt["schema"] != "airk.saved-source-verification/1" { continue; }
        let receipt: airk_core::verification::VerificationReceipt =
            serde_json::from_value(receipt.clone()).context("malformed comparison verification receipt")?;
        let allocation: airk_core::combined_reads::MemberAllocation =
            serde_json::from_value(member["allocation"].clone())
                .context("verification receipt missing its admitted member allocation")?;
        let index: IndexId = serde_json::from_value(member["index_id"].clone())?;
        ensure!(allocation.index_id == index, "crossed verification member allocation");
        ensure!(allocation.verification_deadline_ms <= airk_core::verification::MAX_DEADLINE_MS
            && receipt.limits.deadline_ms <= allocation.verification_deadline_ms
            && receipt.limits.effective_deadline_ms <= receipt.limits.deadline_ms,
            "verification receipt exceeded its admitted deadline");
        ensure!(receipt.limits.max_sources <= allocation.verification_max_sources
            && receipt.limits.max_bytes <= allocation.verification_max_bytes,
            "verification receipt exceeded its admitted source/byte budget");
        let limits = &mut member["result"]["source_verification"]["limits"];
        limits["deadline_ms"] = json!(0);
        limits["effective_deadline_ms"] = json!(0);
    }
    Ok(())
}

'''
assert source[path].count(anchor)==1; source[path]=source[path].replace(anchor,helper+anchor)
source[path]+='''

#[test]
fn comparison_normalizes_only_validated_member_verification_elapsed_windows() -> Result<()> {
    use airk_core::verification::{VerificationBudget, VerificationCounts, VerificationLimits, VerificationMode, VerificationReceipt};
    let index = IndexId::from_identity(Incarnation::from_bytes([71; 16]));
    let allocation = airk_core::combined_reads::MemberAllocation {
        index_id: index, limit: 1, max_bytes: 4096, deadline_ms: 30_000,
        verification_max_sources: 8, verification_max_bytes: 16 << 20,
        verification_deadline_ms: 2000,
    };
    let receipt = VerificationReceipt {
        schema: "airk.saved-source-verification/1".into(), method: "sha256-full-saved-source/1".into(),
        mode: VerificationMode::Report, status: "verified".into(),
        scope_digest: Sha256Digest::from_bytes([72;32]), selection_digest: Sha256Digest::from_bytes([73;32]),
        observed_from_unix_ms: 1, observed_to_unix_ms: 2, publication_fence: "current".into(),
        inventory_complete: true, all_references_accounted: true, source_rows_omitted: 0,
        requirements_satisfied: Some(true), counts: VerificationCounts::default(),
        limits: VerificationLimits::from(VerificationBudget {
            mode: VerificationMode::Report, max_sources: 8, max_bytes: 16 << 20, deadline_ms: 2000,
        }), origins: vec![], sources: vec![], references: vec![], qualifications: vec![], error: None,
    };
    let raw = json!({"schema":"airk.combined-read/1", "members":[{
        "index_id":index,"allocation":allocation,"result":{
            "source_verification":receipt,
            "citation":{"path":"allowed.rs","sha256":Sha256Digest::from_bytes([74;32])},
            "quota":{"used_bytes":7,"reserved_bytes":11},
            "deadline_ms":17
        }}]});
    let unchanged = raw.clone();
    let mut slower = raw.clone();
    slower["members"][0]["result"]["source_verification"]["limits"]["deadline_ms"] = json!(1999);
    slower["members"][0]["result"]["source_verification"]["limits"]["effective_deadline_ms"] = json!(1998);
    let mut expected = raw.clone();
    normalize_member_verification_timing(&mut expected)?;
    normalize_member_verification_timing(&mut slower)?;
    assert_eq!(expected, slower);
    assert_eq!(raw, unchanged, "actual received reply was changed");
    for (pointer, invalid) in [
        ("/members/0/result/source_verification/limits/deadline_ms", json!(2001)),
        ("/members/0/result/source_verification/limits/effective_deadline_ms", json!(2001)),
        ("/members/0/result/source_verification/limits/max_sources", json!(9)),
        ("/members/0/result/source_verification/limits/max_bytes", json!((16 << 20)+1)),
        ("/members/0/allocation/index_id", json!(IndexId::from_identity(Incarnation::from_bytes([75;16])))),
        ("/members/0/allocation/verification_deadline_ms", json!(1999)),
    ] {
        let mut crossed = raw.clone();
        *crossed.pointer_mut(pointer).context("negative fixture pointer absent")? = invalid;
        assert!(normalize_member_verification_timing(&mut crossed).is_err(), "accepted {pointer}");
    }
    // Source identities, outcomes, fixed allowances, quota and unrelated deadline
    // fields are semantic observations; valid timing normalization must retain them.
    for (pointer, changed) in [
        ("/members/0/result/citation/path", json!("crossed.rs")),
        ("/members/0/result/citation/sha256", json!(Sha256Digest::from_bytes([76;32]))),
        ("/members/0/result/source_verification/status", json!("requirements_unmet")),
        ("/members/0/result/source_verification/limits/hit", json!(["deadline"])),
        ("/members/0/result/source_verification/limits/max_sources", json!(7)),
        ("/members/0/result/source_verification/limits/max_bytes", json!((16 << 20)-1)),
        ("/members/0/result/quota/reserved_bytes", json!(12)),
        ("/members/0/result/deadline_ms", json!(18)),
        ("/members/0/allocation/verification_deadline_ms", json!(2001)),
    ] {
        let mut crossed = raw.clone();
        *crossed.pointer_mut(pointer).context("semantic fixture pointer absent")? = changed;
        normalize_member_verification_timing(&mut crossed)?;
        assert_ne!(expected, crossed, "erased {pointer}");
    }
    Ok(())
}
'''
path=paths[1]
anchor='The frontend and one-shot CLI share response observation code.'
note='''The combined native/CLI/MCP parity fixture authenticates each complete raw
delivery digest and materializes its canonical member data before comparison.
Separate requests may report different remaining verification windows after
metadata I/O and sub-millisecond clock truncation. Only each known member
`source_verification.limits.deadline_ms` and `effective_deadline_ms` are treated
as elapsed timing observations after validating `effective <= deadline <=` that
member's admitted `allocation.verification_deadline_ms`, its member identity and
source/byte bounds. The admitted verification allocation, source outcomes,
counts, hashes, limits hit, fixed budgets, quota observations and raw receipts
remain literal; no production deadline or source-check budget is increased.

'''
assert source[path].count(anchor)==1; source[path]=source[path].replace(anchor,note+anchor)
for p in paths:
    dest=out/p; dest.parent.mkdir(parents=True,exist_ok=True); dest.write_text(source[p],encoding='utf-8',newline='\n')
(out/'baseline.json').write_text(json.dumps({p:hashlib.sha256((base/p).read_bytes()).hexdigest() for p in paths},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'files':len(paths),'out':str(out)}))
