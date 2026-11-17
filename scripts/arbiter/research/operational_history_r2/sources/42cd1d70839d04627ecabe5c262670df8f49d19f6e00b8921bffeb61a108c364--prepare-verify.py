# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib

base = Path(__file__).parent
source = base / 'foundation-source'
out = base / 'verify-source'
changes = {}
name = 'crates/airk-store/src/service_collection_integrity.rs'
original = (source / name).read_text(encoding='utf-8')
updated = original.replace('pub mod census;', 'pub mod census;\n#[path = "service_collection_verify.rs"]\npub mod verify;')
updated = updated.replace('    pub known_damage: Vec<KnownDamage>,', '''    pub known_damage: Vec<KnownDamage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify: Option<verify::VerifyProgress>,''')
updated = updated.replace('let mut known_damage = self.load(namespace)?.map_or_else(Vec::new, |value| value.known_damage);', '''let previous = self.load(namespace)?;
        let verify = previous.as_ref().and_then(|value| value.verify.clone());
        let mut known_damage = previous.map_or_else(Vec::new, |value| value.known_damage);''')
updated = updated.replace('            privacy };', '            privacy, verify };')
updated = updated.replace('''        self.publish(read, Some(KnownDamage {
            incarnation: read.head().scope.incarnation, component, observed_unix_ms,
        }), PrivacyEligibility::PlatformEvidenceUnavailable)''', '''        self.bound.revoke_native_integrity();
        let control=self.registry.persist_integrity_damage(self.lease,read.head().scope.incarnation,component,observed_unix_ms);
        let local=self.publish(read, Some(KnownDamage {
            incarnation: read.head().scope.incarnation, component, observed_unix_ms,
        }), PrivacyEligibility::PlatformEvidenceUnavailable);
        match (local,control) {
            (Ok(value),Ok(()))=>Ok(value),
            (Err(error),Ok(()))=>Err(error.context("collection damage receipt failed; control veto was persisted")),
            (Ok(_),Err(error))=>Err(error.context("control veto failed; collection damage receipt was persisted")),
            (Err(error),Err(control))=>Err(error.context(format!("neither damage store confirmed persistence; control failure: {control:#}"))),
        }''')
assert updated != original and 'privacy, verify' in updated
changes[name] = (original, updated)
name = 'crates/airk-store/src/service_collection_verify.rs'
changes[name] = ('', (base / 'verify.rs').read_text(encoding='utf-8'))
name = 'crates/airk-store/src/service_collection_tests.rs'
old = (source / name).read_text(encoding='utf-8')
changes[name] = (old, old + (base / 'verify-tests.rs').read_text(encoding='utf-8'))
name = 'crates/airk-kernel/src/diagnostics.rs'
old = (Path.cwd() / name).read_text(encoding='utf-8')
changes[name] = (old, old + (base / 'verify-kernel.rs').read_text(encoding='utf-8'))
name = 'crates/airk-cli/src/project_worker.rs'
old = (Path.cwd() / name).read_text(encoding='utf-8')
needle = '                        let outcome = execute(&kernel, kind, job.id)?;'
assert old.count(needle) == 1
new = old.replace(needle, '''                        if kind == JobKind::Verify {
                            loop {
                                airk_kernel::service_execution::checkpoint()?;
                                let turn = kernel.verify_service_turn(&registry, &job_lease, &job_storage, job.id)?;
                                if turn.blocked_large {
                                    audit_drain=turn.drained;
                                    return Ok(JobOutcome::Failed{code:"integrity_coverage_incomplete".into()});
                                }
                                ensure!(!turn.progress.requires_repair(), "retained verification has unresolved observations; repair required");
                                if turn.progress.complete() { break; }
                                // Existing job admission/cancellation owns this work;
                                // no parallel scheduler or query-time audit is added.
                                std::thread::yield_now();
                            }
                            return Ok(JobOutcome::Succeeded {
                                revision: kernel.status()?.publication.map(|value| value.token),
                            });
                        }
''' + needle)
new = new.replace('                let mut failure_detail = None;',
                  '                let mut failure_detail = None;\n                let mut audit_drain = None;')
new = new.replace('''                let outcome = if registry.cancellation_requested(job.id)? {
                    Ok(JobOutcome::Canceled)''', '''                let outcome = if registry.cancellation_requested(job.id)? {
                    if job.kind == JobKind::Verify {
                        audit_drain = registry.drained_verify_before_execution(&job_lease,&job_storage,job.id).ok();
                    }
                    Ok(JobOutcome::Canceled)''')
new = new.replace('''                .unwrap_or_else(|error| {
                    failure_detail''', '''                .unwrap_or_else(|error| {
                    audit_drain = airk_store::service::registry::AuditDrainProof::from_error(&error);
                    failure_detail''')
new = new.replace('registry.complete_job_with_maintenance_at(', 'registry.complete_job_with_maintenance_and_audit_at(')
new = new.replace('''                        completed_maintenance.as_ref(),
                    )?;''', '''                        completed_maintenance.as_ref(),
                        audit_drain.as_ref(),
                    )?;''')
start = new.index('        JobKind::Verify => {', new.index('fn execute('))
end = new.index('        JobKind::Recover', start)
new = new[:start] + '        JobKind::Verify => unreachable!("service Verify owns its durable continuation"),\n' + new[end:]
changes[name] = (old, new)
patch = []
for name, (old, new) in changes.items():
    dest = out / name
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(new, encoding='utf-8', newline='\n')
    patch.extend(difflib.unified_diff(old.splitlines(True), new.splitlines(True),
                 fromfile='a/' + name if old else '/dev/null', tofile='b/' + name))
(base / 'verify-after-foundation.patch').write_text(''.join(patch), encoding='utf-8', newline='\n')
