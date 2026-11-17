# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib

base = Path(__file__).parent
root = Path.cwd()
foundation = base / 'foundation-source'
changes = {}
def old_file(name):
    path = foundation / name
    return (path if path.exists() else root / name).read_text(encoding='utf-8')

name = 'crates/airk-store/src/service_registry.rs'
old = old_file(name)
needle = '#[path = "service_jobs.rs"]\nmod jobs;'
assert old.count(needle) == 1
changes[name] = (old, old.replace(needle, needle + '\n#[path = "service_integrity_guard.rs"]\nmod integrity_guard;\npub use integrity_guard::{AuditDrainProof, IntegrityAuditState};'))

name = 'crates/airk-store/src/service_data_scope.rs'
old = old_file(name)
new = old.replace('pub struct BoundDataRoot(Scope);', '''pub struct BoundDataRoot(Scope, LiveIntegrity);
#[derive(Clone, Debug)]
pub(crate) struct LiveIntegrity(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl LiveIntegrity {
    pub(crate) fn revoked(&self)->bool {self.0.load(std::sync::atomic::Ordering::Acquire)}
}''')
new = new.replace('BoundDataRoot(scope)', 'BoundDataRoot(scope, LiveIntegrity(Default::default()))')
new = new.replace('impl BoundDataRoot {', '''impl BoundDataRoot {
    pub(crate) fn live_integrity(&self)->LiveIntegrity {self.1.clone()}
    pub(crate) fn native_integrity_revoked(&self)->bool {self.1.revoked()}
    pub(crate) fn native_control_root(&self)->&Path {&self.0.control}
    pub(crate) fn revoke_native_integrity(&self) {self.1.0.store(true,std::sync::atomic::Ordering::Release);}
''', 1)
assert new != old
changes[name] = (old, new)

name = 'crates/airk-store/src/service_jobs.rs'
old = old_file(name)
needle = '        let claimed = record(&transaction, id.parse()?)?;'
assert old.count(needle) == 1
new = old.replace(needle, needle + '\n        super::integrity_guard::claimed(&transaction, &claimed)?;')
needle = '        let state = match outcome {'
assert new.count(needle) == 1
new = new.replace(needle, '        super::integrity_guard::completed(&transaction, &current, &outcome, drained)?;\n' + needle)
new = new.replace('self.complete_job_observed(lease, id, outcome, None, None)',
                  'self.complete_job_observed(lease, id, outcome, None, None, None)')
new = new.replace('self.complete_job_observed(lease, id, outcome, Some(now_ms), None)',
                  'self.complete_job_observed(lease, id, outcome, Some(now_ms), None, None)')
new = new.replace('self.complete_job_observed(lease, id, outcome, Some(now_ms), maintenance)',
                  'self.complete_job_observed(lease, id, outcome, Some(now_ms), maintenance, None)')
needle = '    fn complete_job_observed('
method = '''    pub fn complete_job_with_maintenance_and_audit_at(
        &mut self, lease: &ProjectLease, id: Sha256Digest, outcome: JobOutcome, now_ms: u64,
        maintenance: Option<&airk_core::service::MaintenanceObservation>,
        drained: Option<&super::AuditDrainProof>,
    ) -> Result<JobRecord> {
        self.complete_job_observed(lease,id,outcome,Some(now_ms),maintenance,drained)
    }

'''
new = new.replace(needle, method + needle)
needle = '''        maintenance: Option<&airk_core::service::MaintenanceObservation>,
    ) -> Result<JobRecord> {
        if let JobOutcome::Failed'''
assert new.count(needle) == 1
new = new.replace(needle, '''        maintenance: Option<&airk_core::service::MaintenanceObservation>,
        drained: Option<&super::AuditDrainProof>,
    ) -> Result<JobRecord> {
        if let JobOutcome::Failed''')
changes[name] = (old, new)

name = 'crates/airk-store/src/service_project_removal.rs'
old = old_file(name)
needle = '"DELETE FROM service_recovery_budget WHERE scope=?1 OR scope=?2",\n            params![format!("source:{project}"), format!("work:{project}")],'
assert old.count(needle) == 1
new = old.replace(needle, '"DELETE FROM service_recovery_budget WHERE scope=?1 OR scope=?2 OR scope=?3 OR scope=?4",\n            params![format!("source:{project}"), format!("work:{project}"), format!("integrity:{project}"), format!("integrity_damage:{project}")],')
changes[name] = (old, new)

name = 'crates/airk-store/src/service_data_relocation.rs'
old = old_file(name)
needle = '        replacements.insert(existing.binding.to_string(), next.binding.to_string());'
assert old.count(needle) == 1
new = old.replace(needle, needle + '''
        replacements.insert(format!("integrity:{}",existing.binding),format!("integrity:{}",next.binding));
        replacements.insert(format!("integrity_damage:{}",existing.binding),format!("integrity_damage:{}",next.binding));''')
changes[name] = (old, new)

name = 'crates/airk-store/src/service_integrity_guard.rs'
changes[name] = ('', (base / 'audit-admission.rs').read_text(encoding='utf-8'))
patch = []
for name, (old, new) in changes.items():
    if old:
        original = base / 'audit-admission-base' / name
        original.parent.mkdir(parents=True, exist_ok=True)
        original.write_text(old, encoding='utf-8', newline='\n')
    dest = base / 'audit-admission-source' / name
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(new, encoding='utf-8', newline='\n')
    patch.extend(difflib.unified_diff(old.splitlines(True), new.splitlines(True),
                 fromfile='a/' + name if old else '/dev/null', tofile='b/' + name))
(base / 'audit-admission-after-foundation.patch').write_text(''.join(patch), encoding='utf-8', newline='\n')
