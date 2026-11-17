# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json
p=Path('artifacts/optimizations/f11-consolidated');name='crates/airk-store/src/service_maintenance.rs';live=Path(name)
record=json.loads((p/'BASE.json').read_text(encoding='utf-8'));record['files'][name]={'sha256':hashlib.sha256(live.read_bytes()).hexdigest(),'layer':'live'}
for layer in ['base','source']:
 dst=p/layer/name;dst.parent.mkdir(parents=True,exist_ok=True);dst.write_bytes(live.read_bytes())
(p/'BASE.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
f=p/'source'/name;s=f.read_text(encoding='utf-8');pos=s.index('pub(super) fn job_recovers_publication')
s=s[:pos]+'''/// Identify physical Verify through its actual immutable owned plan membership.
/// Called inside claim/completion transactions; never infer it from editor intent.
pub(super) fn job_is_physical_verify(connection:&Connection,job:Sha256Digest)->Result<bool>{
    let owners=linked_items(connection,job)?;
    ensure!(owners.len()==1,"physical Repair job requires one native plan owner");
    let (plan,project,step)=owners[0];
    let member=item(connection,plan,project)?;
    ensure!(step==PlanStep::Action&&member.action_job==Some(job),"foreign physical Repair membership");
    Ok(member.action==Some(airk_core::service_maintenance::RepairOperation::Verify))
}
'''+s[pos:];f.write_text(s,encoding='utf-8',newline='\n')
f=p/'source/crates/airk-store/src/service_integrity_guard.rs';s=f.read_text(encoding='utf-8');pos=s.index('fn scope(project:')
s=s[:pos]+'''pub enum AuditEvidence<'a>{
    Drained(&'a AuditDrainProof),
    Complete(&'a crate::service::collection::integrity::verify::AuditCompletionProof),
}
fn physical_audit(connection:&Connection,job:&JobRecord)->Result<bool>{
    Ok(job.kind==JobKind::Verify||(job.kind==JobKind::Repair&&super::maintenance::job_is_physical_verify(connection,job.id)?))
}
'''+s[pos:]
s=s.replace('record.kind==JobKind::Verify && record.project==lease.identity().0','physical_audit(&registry.connection,&record)? && record.project==lease.identity().0')
s=s.replace('    pub(super) fn matches(&self,connection:', '    pub(crate) fn matches(&self,connection:')
s=s.replace('    if job.kind != JobKind::Verify { return Ok(()); }','    if !physical_audit(transaction,job)? { return Ok(()); }',1)
s=s.replace('    drained:Option<&AuditDrainProof>) -> Result<()> {','    evidence:Option<AuditEvidence<\'_>>) -> Result<()> {',1)
s=s.replace('    if job.kind == JobKind::Verify && matches!(outcome, JobOutcome::Succeeded { .. }) {','''    if !physical_audit(transaction,job)? {return Ok(());}
    if matches!(outcome,JobOutcome::Succeeded{..}) {
        let Some(AuditEvidence::Complete(proof))=evidence else {anyhow::bail!("physical audit success requires actual complete proof")};
        ensure!(proof.matches(transaction,job)?,"foreign or revoked audit completion");''',1)
s=s.replace('    } else if job.kind==JobKind::Verify {','    } else {',1)
s=s.replace('        let safe_cancel=clean_terminal && drained.map(|proof|proof.matches(transaction,job)).transpose()?.unwrap_or(false);','        let drained=match evidence{Some(AuditEvidence::Drained(proof))=>Some(proof),_=>None};\n        let safe_cancel=clean_terminal && drained.map(|proof|proof.matches(transaction,job)).transpose()?.unwrap_or(false);',1)
s=s.replace('record.kind==JobKind::Verify\n            && record.state==JobState::Running', 'physical_audit(&self.connection,&record).unwrap_or(false)\n            && record.state==JobState::Running')
f.write_text(s,encoding='utf-8',newline='\n')
f=p/'source/crates/airk-store/src/service_registry.rs';s=f.read_text(encoding='utf-8').replace('pub use integrity_guard::{AuditDrainProof, IntegrityAuditState};','pub use integrity_guard::{AuditDrainProof, AuditEvidence, IntegrityAuditState};');f.write_text(s,encoding='utf-8',newline='\n')
f=p/'source/crates/airk-store/src/service_jobs.rs';s=f.read_text(encoding='utf-8').replace('drained: Option<&super::AuditDrainProof>','evidence: Option<super::AuditEvidence<\'_>>').replace('maintenance,drained)','maintenance,evidence)').replace('outcome, drained)?','outcome, evidence)?');f.write_text(s,encoding='utf-8',newline='\n')
f=p/'source/crates/airk-store/src/service_collection_verify.rs';s=f.read_text(encoding='utf-8');pos=s.index('pub struct VerifyTurn {')
s=s[:pos]+'''/// Non-Clone/non-serializable: constructed only after this module actually
/// verifies complete components, drains owned reads and persists progress.
pub struct AuditCompletionProof {
    drained:crate::service::registry::AuditDrainProof,
    head:CurrentHead,
}
impl AuditCompletionProof {
    pub(crate) fn matches(&self,connection:&rusqlite::Connection,job:&airk_core::service::JobRecord)->Result<bool>{
        self.drained.matches(connection,job)
    }
    pub fn revision(&self)->Result<airk_core::identity::RevisionToken>{
        let token=self.head.token()?;
        Ok(airk_core::identity::RevisionToken{incarnation:token.incarnation,generation:token.sequence,revision:token.digest})
    }
}
'''+s[pos:]
s=s.replace('    pub progress: VerifyProgress,','    pub progress: VerifyProgress,\n    pub completed:Option<AuditCompletionProof>,',1)
s=s.replace('        Ok(VerifyTurn { progress, blocked_large, drained })','''        let completed=if progress.complete() && !blocked_large {
            Some(AuditCompletionProof{drained:crate::service::registry::AuditDrainProof::new(self.registry,self.lease,self.bound,job)?,head:state_head_for_completion})
        }else{None};
        Ok(VerifyTurn { progress, blocked_large, drained, completed })''',1)
# Preserve authenticated exact audited head before persistence moves it into sidecar.
s=s.replace('        let mut blocked_large = false;','        let state_head_for_completion=head.clone();\n        let mut blocked_large = false;',1)
f.write_text(s,encoding='utf-8',newline='\n')
