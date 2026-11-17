# SPDX-License-Identifier: Apache-2.0
from pathlib import Path

root = Path(__file__).parent / 'source/crates/airk-store/src'
def change(name, before, after):
    p = root / name
    text = p.read_text(encoding='utf-8')
    if text.count(before) != 1:
        raise RuntimeError(f'{name}: expected exactly one replacement: {before[:80]}')
    p.write_text(text.replace(before, after), encoding='utf-8', newline='\n')

change('service_collection_integrity.rs',
    '    pub verify: Option<verify::VerifyProgress>,',
    '    pub verify: Option<verify::VerifyProgress>,\n    #[serde(default, skip_serializing_if = "Option::is_none")]\n    pub recovery_verify: Option<verify::RecoveryProgress>,')
change('service_collection_integrity.rs',
    '        let verify = previous.as_ref().and_then(|value| value.verify.clone());',
    '        let verify = previous.as_ref().and_then(|value| value.verify.clone());\n        let recovery_verify = previous.as_ref().and_then(|value| value.recovery_verify.clone());')
change('service_collection_integrity.rs', '            verify,', '            verify,\n            recovery_verify,')

change('service_collection_verify.rs', 'pub struct VerifyTurn {', '''/// Fixed native consumer policy. Projected coverage cannot be minted by this
/// baseline producer; F13 must add its actual predicates before enabling it.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RecoveryProfile { BaselineV1, ProjectedV1 }
pub(crate) const NATIVE_RECOVERY_PROFILE: RecoveryProfile = RecoveryProfile::BaselineV1;

/// Opaque owning recovery purpose. Serialized only as saved audit context, never
/// accepted as authority from a caller. Its live instance comes from the owner.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecoveryEpoch {
    pub(crate) namespace: Incarnation,
    pub(crate) project: airk_core::service::ProjectBinding,
    pub(crate) worker: Incarnation,
    pub(crate) worker_epoch: u64,
    pub(crate) job: Sha256Digest,
    pub(crate) inherited: Sha256Digest,
    pub(crate) head: CurrentHead,
    pub(crate) root: Sha256Digest,
    pub(crate) selected: Sha256Digest,
    pub(crate) profile: RecoveryProfile,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryProgress {
    epoch: RecoveryEpoch,
    progress: VerifyProgress,
}
/// Actual post-epoch coverage, not ordinary historical/global job completion.
/// No constructor, Clone or Deserialize is exposed for physical evidence.
pub struct EnabledComponentAuditProof {
    drained: crate::service::registry::AuditDrainProof,
    epoch: RecoveryEpoch,
    head: CurrentHead,
}
impl EnabledComponentAuditProof {
    pub(crate) fn exact_head(&self) -> &CurrentHead { &self.head }
    pub(crate) fn epoch(&self) -> &RecoveryEpoch { &self.epoch }
    pub(crate) fn matches(&self, connection: &rusqlite::Connection,
        job: &airk_core::service::JobRecord) -> Result<bool> {
        Ok(self.drained.matches(connection, job)?
            && self.epoch.job == job.id
            && self.epoch.profile == NATIVE_RECOVERY_PROFILE
            && self.epoch.head == self.head
            && crate::service::registry::owner_session::recovery_matches(connection, &self.epoch)?)
    }
}
pub struct VerifyTurn {''')
change('service_collection_verify.rs',
    '    pub completed: Option<AuditCompletionProof>,',
    '    pub completed: Option<AuditCompletionProof>,\n    pub recovery_completed: Option<EnabledComponentAuditProof>,')
needle = '''    ) -> Result<VerifyTurn> {
        ensure!(
            (1..=128).contains(&max_files)'''
change('service_collection_verify.rs', needle, '''    ) -> Result<VerifyTurn> {
        self.verify_turn_inner(writer, job, max_files, max_bytes, now, checkpoint, None)
    }
    /// Exceptional owner recovery only. Ordinary Verify retains its dated cursor.
    pub fn verify_recovery_turn(&self, writer: &mut crate::WriterLock,
        epoch: &RecoveryEpoch, max_files: usize, max_bytes: u64, now: u64,
        checkpoint: impl Fn() -> Result<()>) -> Result<VerifyTurn> {
        self.verify_turn_inner(writer, epoch.job, max_files, max_bytes, now, checkpoint, Some(epoch))
    }
    fn verify_turn_inner(&self, writer: &mut crate::WriterLock, job: Sha256Digest,
        max_files: usize, max_bytes: u64, now: u64, checkpoint: impl Fn() -> Result<()>,
        recovery: Option<&RecoveryEpoch>) -> Result<VerifyTurn> {
        ensure!(
            (1..=128).contains(&max_files)''')
change('service_collection_verify.rs',
    '        let old = observation.as_mut().and_then(|value| value.verify.take());',
    '''        if let Some(epoch) = recovery {
            self.registry.require_recovery_epoch(self.lease, epoch)?;
            ensure!(epoch.head == head, "recovery publication changed");
        }
        // Never borrow ordinary/historical pages for exceptional recovery. A
        // new admitted epoch, owner exposure, purpose or head starts at file 0
        // and repeats the lexical component; old dates remain in state.verify.
        let old = if let Some(epoch) = recovery {
            observation.as_mut().and_then(|value| value.recovery_verify.take())
                .filter(|saved| &saved.epoch == epoch).map(|saved| saved.progress)
        } else {
            observation.as_mut().and_then(|value| value.verify.take())
        };''')
change('service_collection_verify.rs',
    '            state.verify = Some(progress.clone());',
    '''            if let Some(epoch) = recovery {
                self.registry.require_recovery_epoch(self.lease, epoch)?;
                state.recovery_verify = Some(RecoveryProgress { epoch: epoch.clone(), progress: progress.clone() });
            } else {
                state.verify = Some(progress.clone());
            }''')
change('service_collection_verify.rs',
    '                head: state_head_for_completion,',
    '                head: state_head_for_completion.clone(),')
change('service_collection_verify.rs', '        Ok(VerifyTurn {', '''        let recovery_completed = if progress.complete() && !blocked_large {
            if let Some(epoch) = recovery {
                self.registry.require_recovery_epoch(self.lease, epoch)?;
                Some(EnabledComponentAuditProof {
                    drained: crate::service::registry::AuditDrainProof::new(self.registry, self.lease, self.bound, job)?,
                    epoch: epoch.clone(), head: state_head_for_completion,
                })
            } else { None }
        } else { None };
        Ok(VerifyTurn {''')
change('service_collection_verify.rs', '            completed,', '            completed,\n            recovery_completed,')

change('service_collection_native_owner.rs',
    '    fn locked_native(&self) -> Result<LockedPublication> {',
    '''    fn locked_native(&self) -> Result<LockedPublication> { self.locked_native_inner(false) }
    fn locked_native_inner(&self, recovery: bool) -> Result<LockedPublication> {''')
change('service_collection_native_owner.rs', '        self.native_no_damage(&head)?;', '        self.native_no_damage(&head, recovery)?;')
change('service_collection_native_owner.rs', '    fn native_no_damage(&self, head: &CurrentHead) -> Result<()> {', '    fn native_no_damage(&self, head: &CurrentHead, recovery: bool) -> Result<()> {')
change('service_collection_native_owner.rs', '            !matches!(\n                self.registry.integrity_audit_state(self.lease)?,', '            recovery || !matches!(\n                self.registry.integrity_audit_state(self.lease)?,')
change('service_collection_native_owner.rs',
    '        proof: &super::verify::AuditCompletionProof,',
    '        proof: &super::verify::EnabledComponentAuditProof,')
change('service_collection_native_owner.rs', '''        let locked = owner.locked_native()?;
        ensure!(
            proof.exact_head() == &locked.head,''', '''        let locked = owner.locked_native_inner(true)?;
        ensure!(
            proof.epoch().worker == identity.worker
                && proof.epoch().worker_epoch == fence.1
                && proof.epoch().root == digest(b"native-root/1", &locked.root.id()?)?
                && proof.epoch().selected == digest(b"native-selected/1", &locked.selected.id()?)?
                && proof.exact_head() == &locked.head,''')
# Purpose creation uses existing owning locks before registry checks; it writes
# no new selection/lock and starts no audit on matching Clean reopen.
with (root/'service_collection_native_owner.rs').open('a', encoding='utf-8') as out:
    out.write('''
impl Collection {
    pub fn begin_native_recovery(&self, registry: &ServiceRegistry, lease: &ProjectLease,
        bound: &BoundDataRoot, identity: WorkerIdentity, job: Sha256Digest)
        -> Result<super::verify::RecoveryEpoch> {
        let owner = self.integrity_owner(registry, lease, bound)?;
        let fence = owner.check()?;
        ensure!(identity.service == fence.0 && identity.project == lease.identity().0
            && identity.fencing_generation == fence.1, "foreign native recovery owner");
        let locked = owner.locked_native_inner(true)?;
        let previous = registry.load_owner_session(lease)?.context("exceptional owner session missing")?;
        let epoch = super::verify::RecoveryEpoch {
            namespace: fence.0, project: identity.project, worker: identity.worker,
            worker_epoch: fence.1, job,
            inherited: digest(b"native-recovery-owner/1", &previous.0)?,
            head: locked.head.clone(), root: digest(b"native-root/1", &locked.root.id()?)?,
            selected: digest(b"native-selected/1", &locked.selected.id()?)?,
            profile: super::verify::NATIVE_RECOVERY_PROFILE,
        };
        registry.require_recovery_epoch(lease, &epoch)?;
        Ok(epoch)
    }
}
''')

owner_path = root/'service_owner_session.rs'
owner_text = owner_path.read_text(encoding='utf-8')
assert owner_text.count('verify::AuditCompletionProof') == 2
owner_path.write_text(owner_text.replace('verify::AuditCompletionProof', 'verify::EnabledComponentAuditProof'), encoding='utf-8', newline='\n')
change('service_owner_session.rs', '''        transaction.execute("INSERT INTO service_recovery_budget VALUES''', '''        if let Some((job, _)) = audit {
            // Separate exact enabled-policy recovery transition; generic success
            // and clean partial drain still cannot clear an inherited veto.
            transaction.execute("DELETE FROM service_recovery_budget WHERE scope=?1 AND engine=?2",
                params![format!("integrity:{}", next.project), job.to_string()])?;
        }
        transaction.execute("INSERT INTO service_recovery_budget VALUES''')
with (root/'service_owner_session.rs').open('a', encoding='utf-8') as out:
    out.write('''
pub(crate) fn recovery_matches(connection: &Connection,
    epoch: &crate::service::collection::integrity::verify::RecoveryEpoch) -> Result<bool> {
    let Some((raw, _)) = load(connection, epoch.project)? else { return Ok(false) };
    let inherited = airk_core::identity::framed_digest(b"native-recovery-owner/1", &[&serde_json::to_vec(&raw)?]);
    let namespace: String = connection.query_row("SELECT incarnation FROM service_meta WHERE id=1", [], |row| row.get(0))?;
    Ok(inherited == epoch.inherited && namespace.parse::<Incarnation>()? == epoch.namespace
        && epoch.profile == crate::service::collection::integrity::verify::NATIVE_RECOVERY_PROFILE)
}
impl ServiceRegistry {
    pub(crate) fn require_recovery_epoch(&self, lease: &ProjectLease,
        epoch: &crate::service::collection::integrity::verify::RecoveryEpoch) -> Result<()> {
        let admitted = jobs::worker_epoch(&self.connection, &self.root, lease)?;
        let job = self.job(epoch.job)?;
        ensure!(epoch.project == lease.identity().0 && epoch.worker_epoch == admitted
            && job.project == epoch.project && job.state == airk_core::service::JobState::Running
            && job.fencing_generation == admitted
            && self.integrity_audit_veto(lease)? == Some(epoch.job)
            && recovery_matches(&self.connection, epoch)?, "stale native recovery epoch");
        Ok(())
    }
}
''')
