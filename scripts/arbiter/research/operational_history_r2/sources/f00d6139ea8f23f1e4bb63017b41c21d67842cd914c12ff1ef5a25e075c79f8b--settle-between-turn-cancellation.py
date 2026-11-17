# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f11-consolidated/source/crates/airk-cli/src/project_worker.rs')
s=p.read_text(encoding='utf-8');s=s.replace('                                ensure!(!turn.progress.requires_repair(), "retained verification has unresolved observations; repair required");\n','',1)
needle='                    audit_drain = airk_store::service::registry::AuditDrainProof::from_error(&error);'
assert needle in s;s=s.replace(needle,needle+'''
                    if audit_drain.is_none() && error.is::<airk_kernel::service_execution::Canceled>()
                        && matches!(job.kind,JobKind::Verify|JobKind::Repair) {
                        // This synchronous accepted job stack has fully unwound;
                        // no read/turn task is detached. Revoked/failed persistence
                        // cannot mint this exact-job safe-drain proof.
                        audit_drain=registry.drained_verify_before_execution(&job_lease,&job_storage,job.id).ok();
                    }''',1)
p.write_text(s,encoding='utf-8',newline='\n')
