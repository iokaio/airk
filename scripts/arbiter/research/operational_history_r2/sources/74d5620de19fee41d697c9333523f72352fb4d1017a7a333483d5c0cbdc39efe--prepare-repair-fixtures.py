# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib
p=Path('artifacts/optimizations/f11-consolidated');name='crates/airk-store/src/service_maintenance_tests.rs';live=Path(name)
r=json.loads((p/'BASE.json').read_text(encoding='utf-8'));r['files'][name]={'sha256':hashlib.sha256(live.read_bytes()).hexdigest(),'layer':'live'}
for layer in ['base','source']:
 dst=p/layer/name;dst.parent.mkdir(parents=True,exist_ok=True);dst.write_bytes(live.read_bytes())
(p/'BASE.json').write_text(json.dumps(r,indent=2)+'\n',encoding='utf-8')
f=p/'source'/name;s=f.read_text(encoding='utf-8');start=s.index('fn approved_plan_is_durable_fenced_private_and_preserves_partial_failure');end=s.index('\n#[test]',start)
block=s[start:end]
old='''    registry.complete_job_at(
        &a_lease,
        a_job.id,
        JobOutcome::Succeeded { revision: None },
        1800,
    )?;'''
assert old in block;block=block.replace(old,'''    // A progress row/ordinary success is no longer physical audit evidence.
    assert!(registry.complete_job_at(&a_lease,a_job.id,JobOutcome::Succeeded{revision:None},1800).is_err());
    registry.complete_job_at(&a_lease,a_job.id,JobOutcome::Failed{code:"integrity_coverage_incomplete".into()},1800)?;''',1)
block=block.replace('assert_eq!((completed.counts.succeeded, completed.counts.stale), (1, 1));','assert_eq!((completed.counts.succeeded,completed.counts.failed,completed.counts.stale),(0,1,1));')
s=s[:start]+block+s[end:];f.write_text(s,encoding='utf-8',newline='\n')
f=p/'source/crates/airk-store/src/service_integrity_guard.rs';s=f.read_text(encoding='utf-8');needle='        ensure!(proof.matches(transaction,job)?,"foreign or revoked audit completion");'
s=s.replace(needle,needle+'''
        let JobOutcome::Succeeded{revision:Some(revision)}=outcome else{anyhow::bail!("physical audit success requires exact audited publication")};
        ensure!(*revision==proof.revision()?,"physical audit publication mismatch");''',1);f.write_text(s,encoding='utf-8',newline='\n')
f=p/'source/crates/airk-store/src/service_collection_tests.rs';s=f.read_text(encoding='utf-8');s+='''

#[test]
fn approved_physical_repair_claim_and_success_require_actual_guard_and_component_proof() -> Result<()> {
    use crate::service::registry::{AuditEvidence,IntegrityAuditState};
    use airk_core::{service::{JobKind,JobOutcome,JobState,RequestKey,WorkerIdentity},service_maintenance::{RepairOperation,RepairRequest,RepairSelection},service_inspection::{IndexState,SourceState,InspectionSnapshot}};
    let (_temp,collection,mut registry,bound,lease,store,scope)=integrity_fixture()?;
    let project=lease.identity().0;let (namespace,epoch)=registry.saved_request_epoch()?;
    let engine=Sha256Digest::from_bytes([91;32]);
    let plan=registry.create_maintenance_plan(namespace,&RequestKey{namespace,epoch,nonce:Incarnation::from_bytes([92;16])},
        &RepairRequest{selection:RepairSelection::Selected(vec![project]),operation:RepairOperation::Verify},1000,engine)?;
    registry.advance_maintenance_plans(1100)?;
    let inspect=registry.claim_job(&lease)?.context("plan inspection missing")?;assert_eq!(inspect.kind,JobKind::PlanInspect);
    let token=store.current_read(&scope)?.token()?;
    let revision=airk_core::identity::RevisionToken{incarnation:token.incarnation,generation:token.sequence,revision:token.digest};
    let snapshot=InspectionSnapshot{quota_policy_digest:None,project,incarnation:scope.incarnation,publication:Some(revision),settings_digest:Some(engine),
        index_state:IndexState::Ready,source_state:SourceState::Available,recovery_required:Some(false),indexed_files:Some(0),indexed_vectors:Some(0),vectors_enabled:Some(false),index_detail:None,semantic:None,discovery:None,failure:None,storage:None};
    registry.record_inspection(&lease,WorkerIdentity{service:namespace,worker:Incarnation::from_bytes([93;16]),project,fencing_generation:inspect.fencing_generation},inspect.id,1200,&snapshot)?;
    registry.complete_job_at(&lease,inspect.id,JobOutcome::Succeeded{revision:Some(revision)},1300)?;
    let ready=registry.maintenance_plan(plan.id)?;registry.approve_maintenance_plan(plan.id,ready.preview_digest.context("preview missing")?,false,1400)?;
    registry.advance_maintenance_plans(1500)?;
    let queued=registry.pending_job_observation(project)?.context("physical Repair missing")?;assert_eq!(queued.kind,JobKind::Repair);
    registry.connection.execute_batch("CREATE TEMP TRIGGER refuse_physical_repair_guard BEFORE INSERT ON service_recovery_budget BEGIN SELECT RAISE(ABORT,'injected'); END;")?;
    assert!(registry.claim_job_id(&lease,queued.id).is_err());assert_eq!(registry.job(queued.id)?.state,JobState::Pending);
    registry.connection.execute_batch("DROP TRIGGER refuse_physical_repair_guard;")?;
    let claimed=registry.claim_job_id(&lease,queued.id)?.context("physical Repair not claimed")?;
    assert_eq!(registry.integrity_audit_state(&lease)?,IntegrityAuditState::LiveAudit{job:claimed.id});
    assert!(registry.complete_job(&lease,claimed.id,JobOutcome::Succeeded{revision:Some(revision)}).is_err());
    let complete={let owner=collection.integrity_owner(&registry,&lease,&bound)?;let read=store.current_read(&scope)?;owner.anchor(&read)?;drop(read);
        let mut writer=store.lock()?;let turn=owner.verify_turn(&mut writer,claimed.id,1,1<<20,1600,||Ok(()))?;
        assert!(turn.progress.complete());turn.completed.context("physical completion proof missing")?};
    let wrong=airk_core::identity::RevisionToken{revision:engine,..complete.revision()?};
    assert!(registry.complete_job_with_maintenance_and_audit_at(&lease,claimed.id,JobOutcome::Succeeded{revision:Some(wrong)},1700,None,Some(AuditEvidence::Complete(&complete))).is_err());
    assert_eq!(registry.job(claimed.id)?.state,JobState::Running);
    registry.complete_job_with_maintenance_and_audit_at(&lease,claimed.id,JobOutcome::Succeeded{revision:Some(complete.revision()?)},1800,None,Some(AuditEvidence::Complete(&complete)))?;
    assert_eq!(registry.integrity_audit_state(&lease)?,IntegrityAuditState::Clear);
    assert_eq!(registry.maintenance_plan(plan.id)?.counts.succeeded,1);
    Ok(())
}
''';f.write_text(s,encoding='utf-8',newline='\n')
