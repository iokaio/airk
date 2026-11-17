# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f11-consolidated/source')
f=p/'crates/airk-cli/src/project_worker.rs';s=f.read_text(encoding='utf-8')
s=s.replace('                let mut audit_drain = None;', '                let mut audit_drain = None;\n                let mut audit_completion = None;',1)
s=s.replace('if job.kind == JobKind::Verify {\n                        audit_drain', 'if matches!(job.kind,JobKind::Verify|JobKind::Repair) {\n                        audit_drain',1)
old='''                                if turn.progress.complete() { break; }'''
new='''                                if turn.progress.complete() {
                                    let complete=turn.completed.context("complete audit lacks physical proof")?;
                                    let revision=complete.revision()?;
                                    audit_completion=Some(complete);
                                    return Ok(JobOutcome::Succeeded{revision:Some(revision)});
                                }'''
assert old in s;s=s.replace(old,new,1)
old='''                            return Ok(JobOutcome::Succeeded {
                                revision: kernel.status()?.publication.map(|value| value.token),
                            });
'''
assert old in s;s=s.replace(old,'',1)
s=s.replace('                        audit_drain.as_ref(),','''                        if matches!(outcome,JobOutcome::Succeeded{..}) {
                            audit_completion.as_ref().map(airk_store::service::registry::AuditEvidence::Complete)
                        }else{
                            audit_drain.as_ref().map(airk_store::service::registry::AuditEvidence::Drained)
                        },''',1)
f.write_text(s,encoding='utf-8',newline='\n')
# Keep historical fixture APIs coherent with sealed success rather than letting
# generic Succeeded act as an audit certificate.
f=p/'crates/airk-store/src/service_collection_tests.rs';s=f.read_text(encoding='utf-8')
s=s.replace('    {\n        let owner = collection.integrity_owner(&registry, &lease, &bound)?;\n        let mut writer = store.lock()?;\n        let resumed = owner.verify_turn', '    let resumed_complete={\n        let owner = collection.integrity_owner(&registry, &lease, &bound)?;\n        let mut writer = store.lock()?;\n        let resumed = owner.verify_turn',1)
s=s.replace('''    }
    registry.complete_job(&lease, job, JobOutcome::Succeeded { revision: None })?;''','''        resumed.completed.context("resumed physical completion proof missing")?
    };
    assert!(registry.complete_job(&lease,job,JobOutcome::Succeeded{revision:None}).is_err(),"generic success cannot settle audit");
    registry.complete_job_with_maintenance_and_audit_at(&lease,job,JobOutcome::Succeeded{revision:Some(resumed_complete.revision()?)},201,None,Some(crate::service::registry::AuditEvidence::Complete(&resumed_complete)))?;''',1)
s=s.replace('''    {
        let owner = collection.integrity_owner(&registry, &lease, &bound)?;
        let mut writer = store.lock()?;
        let fresh = owner.verify_turn''','''    let fresh_complete={
        let owner = collection.integrity_owner(&registry, &lease, &bound)?;
        let mut writer = store.lock()?;
        let fresh = owner.verify_turn''',1)
s=s.replace('''    }
    registry.complete_job(&lease, fresh_job, JobOutcome::Succeeded { revision: None })?;''','''        fresh.completed.context("fresh physical completion proof missing")?
    };
    registry.complete_job_with_maintenance_and_audit_at(&lease,fresh_job,JobOutcome::Succeeded{revision:Some(fresh_complete.revision()?)},301,None,Some(crate::service::registry::AuditEvidence::Complete(&fresh_complete)))?;''',1)
s=s.replace('Some(&drained))?;', 'Some(crate::service::registry::AuditEvidence::Drained(&drained)))?;')
s=s.replace('clean_turn.drained.as_ref())?;', 'clean_turn.drained.as_ref().map(crate::service::registry::AuditEvidence::Drained))?;')
s=s.replace('replacement_turn.drained.as_ref())?;', 'replacement_turn.drained.as_ref().map(crate::service::registry::AuditEvidence::Drained))?;')
f.write_text(s,encoding='utf-8',newline='\n')
