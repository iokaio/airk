// SPDX-License-Identifier: Apache-2.0
//! Off-query dated observations. No method in this module authorizes a query.
use super::*;
use crate::current::postings::Lookup;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyProgress {
    version: u32,
    job: Sha256Digest,
    scope: crate::current::CurrentScope,
    artifact_root: Sha256Digest,
    lexical_commit: Sha256Digest,
    lexical_observed_unix_ms: Option<u64>,
    started_unix_ms: u64,
    through_unix_ms: u64,
    after: Option<String>,
    checked_files: u64,
    checked_bytes: u64,
    objects_complete: bool,
    /// Includes uncertain I/O; not a declaration of physical corruption.
    /// Metadata/partial observation cannot clear it; actual full clean physical
    /// completion may settle uncertainty after all declared components finish.
    unresolved: bool,
}
impl VerifyProgress {
    pub fn requires_repair(&self) -> bool {
        self.unresolved
    }
    fn reconcile(old: Option<Self>, job: Sha256Digest, head: &CurrentHead, now: u64) -> Self {
        let unresolved = old.as_ref().is_some_and(|old| old.unresolved);
        let Some(mut old) =
            old.filter(|old| old.version == 1 && old.job == job && old.scope == head.scope)
        else {
            return Self {
                version: 1,
                job,
                scope: head.scope.clone(),
                artifact_root: head.artifact_root,
                lexical_commit: head.lexical_commit,
                lexical_observed_unix_ms: None,
                started_unix_ms: now,
                through_unix_ms: now,
                after: None,
                checked_files: 0,
                checked_bytes: 0,
                objects_complete: false,
                unresolved,
            };
        };
        // Metadata/semantic/graph-only publication need not discard unrelated
        // observed object work. These are authenticated component identities.
        if old.artifact_root != head.artifact_root {
            old.artifact_root = head.artifact_root;
            old.after = None;
            old.checked_files = 0;
            old.checked_bytes = 0;
            old.objects_complete = false;
            old.started_unix_ms = now;
        }
        if old.lexical_commit != head.lexical_commit {
            old.lexical_commit = head.lexical_commit;
            old.lexical_observed_unix_ms = None;
        }
        old
    }
    pub fn complete(&self) -> bool {
        self.objects_complete && self.lexical_observed_unix_ms.is_some() && !self.unresolved
    }
}

/// Non-Clone/non-serializable: constructed only after this module actually
/// verifies the declared lexical/retained-input components, drains owned reads
/// and persists dated progress. This is not a certificate for future F13
/// projections, graph/vector bytes or every file in a private data root.
pub struct AuditCompletionProof {
    drained: crate::service::registry::AuditDrainProof,
    head: CurrentHead,
}
impl AuditCompletionProof {
    pub(crate) fn matches(
        &self,
        connection: &rusqlite::Connection,
        job: &airk_core::service::JobRecord,
    ) -> Result<bool> {
        self.drained.matches(connection, job)
    }
    pub fn revision(&self) -> Result<airk_core::identity::RevisionToken> {
        let token = self.head.token()?;
        Ok(airk_core::identity::RevisionToken {
            incarnation: token.incarnation,
            generation: token.sequence,
            revision: token.digest,
        })
    }
}
/// Fixed native consumer policy. Projected coverage cannot be minted by this
/// baseline producer; F13 must add its actual predicates before enabling it.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RecoveryProfile {
    BaselineV1,
    ProjectedV1,
}
pub(crate) const NATIVE_RECOVERY_PROFILE: RecoveryProfile = RecoveryProfile::BaselineV1;

/// Opaque owning recovery purpose. Serialized only as saved audit context, never
/// accepted as authority from a caller. Its live instance comes from the owner.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecoveryContext {
    pub(crate) namespace: Incarnation,
    pub(crate) project: airk_core::service::ProjectBinding,
    pub(crate) worker: Incarnation,
    pub(crate) worker_epoch: u64,
    pub(crate) job: Sha256Digest,
    pub(crate) inherited: Sha256Digest,
    pub(crate) head: CurrentToken,
    pub(crate) root: Sha256Digest,
    pub(crate) selected: Sha256Digest,
    pub(crate) profile: RecoveryProfile,
}
/// Live purpose has no Deserialize constructor. Persisted context is only
/// continuation data and must be checked against this newly minted live value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryEpoch {
    pub(crate) context: RecoveryContext,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryReceipt {
    version: u32,
    owner: Sha256Digest,
    namespace: Incarnation,
    epoch: RecoveryContext,
    progress: VerifyProgress,
}
const RECOVERY_RECEIPT: &str = "integrity-recovery.json";
impl RecoveryReceipt {
    fn validate(&self, owner: Sha256Digest) -> Result<()> {
        ensure!(
            self.version == 1
                && self.owner == owner
                && self.namespace == self.epoch.namespace
                && self.epoch.worker_epoch > 0
                && self.epoch.head.sequence <= i64::MAX as u64
                && self.progress.job == self.epoch.job,
            "foreign or unsupported recovery continuation"
        );
        self.progress.scope.validate()?;
        ensure!(
            self.progress.version == 1
                && self.progress.scope.incarnation == self.epoch.head.incarnation
                && self
                    .progress
                    .after
                    .as_ref()
                    .is_none_or(|path| !path.is_empty()
                        && path.len() <= 8192
                        && !path.contains('\0')),
            "unsupported or malformed recovery continuation"
        );
        Ok(())
    }
}
pub(crate) fn owned_recovery_scratch(path: &Path, owner: Sha256Digest) -> bool {
    json::<RecoveryReceipt>(path).is_ok_and(|receipt| receipt.validate(owner).is_ok())
}
impl IntegrityOwner<'_> {
    fn load_recovery(&self, namespace: Incarnation) -> Result<Option<RecoveryReceipt>> {
        let path = self.collection.directory.join(RECOVERY_RECEIPT);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
            Ok(_) => {}
        }
        let receipt: RecoveryReceipt = json(&path)?;
        receipt.validate(self.collection.binding)?;
        ensure!(
            receipt.namespace == namespace
                && receipt.progress.scope.root == self.collection.owner.root
                && receipt.progress.scope.collection == self.collection.owner.collection,
            "foreign recovery namespace or publication scope"
        );
        Ok(Some(receipt))
    }
}
/// Actual post-epoch coverage, not ordinary historical/global job completion.
/// No constructor, Clone or Deserialize is exposed for physical evidence.
pub struct EnabledComponentAuditProof {
    drained: crate::service::registry::AuditDrainProof,
    epoch: RecoveryContext,
    head: CurrentHead,
}
impl EnabledComponentAuditProof {
    pub(crate) fn exact_head(&self) -> &CurrentHead {
        &self.head
    }
    pub(crate) fn epoch(&self) -> &RecoveryContext {
        &self.epoch
    }
    pub(crate) fn matches(
        &self,
        connection: &rusqlite::Connection,
        job: &airk_core::service::JobRecord,
    ) -> Result<bool> {
        Ok(self.drained.matches(connection, job)?
            && self.epoch.job == job.id
            && self.epoch.profile == NATIVE_RECOVERY_PROFILE
            && self.epoch.head == self.head.token()?
            && crate::service::registry::owner_session::recovery_matches(connection, &self.epoch)?)
    }
}
pub struct VerifyTurn {
    pub verified_files: u64,
    pub progress: VerifyProgress,
    pub completed: Option<AuditCompletionProof>,
    pub recovery_completed: Option<EnabledComponentAuditProof>,
    pub blocked_large: bool,
    pub drained: Option<crate::service::registry::AuditDrainProof>,
}
impl IntegrityOwner<'_> {
    /// One optional legacy lexical component and one bounded retained-object page.
    /// Acquire the existing writer before entry, release it between turns. No
    /// CurrentRead, transaction, writer or object lease survives this call.
    /// `checkpoint` is the existing service job cancellation/admission hook.
    pub fn verify_turn(
        &self,
        writer: &mut crate::WriterLock,
        job: Sha256Digest,
        max_files: usize,
        max_bytes: u64,
        now: u64,
        checkpoint: impl Fn() -> Result<()>,
    ) -> Result<VerifyTurn> {
        self.verify_turn_inner(writer, job, (max_files, max_bytes), now, checkpoint, None)
    }
    /// Exceptional owner recovery only. Ordinary Verify retains its dated cursor.
    pub fn verify_recovery_turn(
        &self,
        writer: &mut crate::WriterLock,
        epoch: &RecoveryEpoch,
        max_files: usize,
        max_bytes: u64,
        now: u64,
        checkpoint: impl Fn() -> Result<()>,
    ) -> Result<VerifyTurn> {
        self.verify_turn_inner(
            writer,
            epoch.context.job,
            (max_files, max_bytes),
            now,
            checkpoint,
            Some(epoch),
        )
    }
    fn verify_turn_inner(
        &self,
        writer: &mut crate::WriterLock,
        job: Sha256Digest,
        limits: (usize, u64),
        now: u64,
        checkpoint: impl Fn() -> Result<()>,
        recovery: Option<&RecoveryEpoch>,
    ) -> Result<VerifyTurn> {
        let (max_files, max_bytes) = limits;
        let recovery = recovery.map(|epoch| &epoch.context);
        ensure!(
            (1..=128).contains(&max_files) && (1..=512 << 20).contains(&max_bytes),
            "invalid off-query verification bounds"
        );
        if let Err(error) = checkpoint() {
            return Err(
                match crate::service::registry::AuditDrainProof::new(
                    self.registry,
                    self.lease,
                    self.bound,
                    job,
                ) {
                    Ok(proof) => error.context(proof),
                    Err(_) => error,
                },
            );
        }
        let fence = self.check()?;
        ensure!(
            self.registry.integrity_audit_veto(self.lease)? == Some(job),
            "Verify requires its durably admitted audit veto"
        );
        let selected = self.collection.select()?;
        crate::maintenance::check_writer(
            &selected.store,
            writer,
            &Quota::open_service(Path::new(&self.collection.owner.data_root))?,
        )?;
        let generation = writer.census_mutation_epoch();
        let scope = selected
            .store
            .current_owned_scope()?
            .context("verification scope missing")?;
        let read = selected.store.current_read(&scope)?;
        let head = read.head().clone();
        let mut observation = {
            let _lock = manifest_lock(&self.collection.directory)?;
            self.load(fence.0)?
        };
        if let Some(epoch) = recovery {
            self.registry.require_recovery_epoch(self.lease, epoch)?;
            ensure!(epoch.head == head.token()?, "recovery publication changed");
        }
        // Recovery never enlarges or rewrites the ordinary receipt. Only an
        // exactly matching live purpose can resume the separate bounded cursor.
        let old = if let Some(epoch) = recovery {
            self.load_recovery(fence.0)?
                .filter(|saved| &saved.epoch == epoch)
                .map(|saved| saved.progress)
        } else {
            observation.as_mut().and_then(|value| value.verify.take())
        };
        if let Some(saved) = &old {
            saved.scope.validate()?;
            ensure!(
                saved.version == 1
                    && saved.after.as_ref().is_none_or(|path| !path.is_empty()
                        && path.len() <= 8192
                        && !path.contains('\0')),
                "unsupported or malformed Verify continuation"
            );
        }
        let mut progress = VerifyProgress::reconcile(old, job, &head, now);
        if observation.as_ref().is_some_and(|value| {
            value.known_damage.len() >= MAX_DAMAGE
                || value
                    .known_damage
                    .iter()
                    .any(|damage| damage.incarnation == head.scope.incarnation)
        }) {
            progress.unresolved = true;
        }
        let recorded = self.registry.recorded_integrity_damage(self.lease)?;
        if recorded.len() >= MAX_DAMAGE
            || recorded
                .iter()
                .any(|damage| damage.incarnation == head.scope.incarnation)
        {
            progress.unresolved = true;
        }
        let state_head_for_completion = head.clone();
        let files_before = progress.checked_files;
        let mut blocked_large = false;
        let mut interrupted = false;
        let mut check = || match checkpoint() {
            Ok(()) => Ok(()),
            Err(error) => {
                interrupted = true;
                Err(error)
            }
        };
        let work = (|| -> Result<()> {
            if progress.lexical_observed_unix_ms.is_none() {
                check()?;
                // Existing v2 auxiliary SHA streams are indivisible; this can
                // take longer than a turn. Never called once per object page.
                read.verify_lexical_integrity()?;
                progress.lexical_observed_unix_ms = Some(now);
            }
            check()?;
            if progress.objects_complete {
                return Ok(());
            }
            let page =
                read.lookup_paths(Lookup::Files, "", progress.after.as_deref(), max_files + 1)?;
            let mut bytes = 0u64;
            let mut visited = 0usize;
            for path in page.paths.iter().take(max_files) {
                check()?;
                let artifact = read
                    .artifact(path)?
                    .context("verification artifact missing")?;
                let reference = artifact
                    .retained
                    .as_ref()
                    .context("verification retained reference missing")?;
                let size = (artifact.bytes as u64)
                    .checked_add(reference.object_bytes as u64)
                    .context("verification byte overflow")?;
                if size > max_bytes {
                    blocked_large = true;
                    break;
                }
                if bytes.saturating_add(size) > max_bytes {
                    break;
                }
                selected.store.verify_retained_source(&artifact)?;
                selected.store.load_retained_bounded(&artifact, max_bytes)?;
                bytes += size;
                progress.checked_files = progress
                    .checked_files
                    .checked_add(1)
                    .context("verification count overflow")?;
                progress.checked_bytes = progress
                    .checked_bytes
                    .checked_add(size)
                    .context("verification size overflow")?;
                progress.after = Some(path.clone());
                visited += 1;
            }
            progress.objects_complete = visited == page.paths.len() && page.next_after.is_none();
            progress.through_unix_ms = now;
            Ok(())
        })();
        // Partial successes precede the failing object; retain them, but never
        // turn an uncertain/failed turn into a complete clean audit.
        if work.is_err() && !interrupted {
            self.bound.revoke_native_integrity();
            progress.unresolved = true;
        }
        drop(read);
        let persistence = (|| -> Result<()> {
            let _lock = manifest_lock(&self.collection.directory)?;
            ensure!(
                self.check()? == fence && writer.census_mutation_epoch() == generation,
                "verification owner changed"
            );
            ensure!(
                self.registry.integrity_audit_veto(self.lease)? == Some(job),
                "verification audit admission changed"
            );
            let manifest = self.collection.manifest()?;
            let current = FileStore::new(self.collection.slot(manifest.active));
            let check = current.current_read(&scope)?;
            ensure!(
                check.token()? == head.token()?,
                "verification publication changed"
            );
            drop(check);
            // Reload to preserve concurrent sticky damage; never overwrite an
            // old saved observation with the earlier in-memory copy.
            let mut state = self
                .load(fence.0)?
                .context("anchor required before Verify")?;
            let recorded = self.registry.recorded_integrity_damage(self.lease)?;
            let known = state.known_damage.len() >= MAX_DAMAGE
                || state
                    .known_damage
                    .iter()
                    .any(|damage| damage.incarnation == head.scope.incarnation)
                || recorded.len() >= MAX_DAMAGE
                || recorded
                    .iter()
                    .any(|damage| damage.incarnation == head.scope.incarnation);
            if known {
                progress.unresolved = true;
            } else if work.is_ok()
                && progress.objects_complete
                && progress.lexical_observed_unix_ms.is_some()
            {
                // Resolves only uncertain/interrupted physical work after its
                // actual complete recheck; never clears either sticky damage list.
                progress.unresolved = false;
            }
            if let Some(epoch) = recovery {
                self.registry.require_recovery_epoch(self.lease, epoch)?;
                let receipt = RecoveryReceipt {
                    version: 1,
                    owner: self.collection.binding,
                    namespace: fence.0,
                    epoch: epoch.clone(),
                    progress: progress.clone(),
                };
                receipt.validate(self.collection.binding)?;
                write(self.collection, RECOVERY_RECEIPT, &receipt)?;
            } else {
                state.verify = Some(progress.clone());
                state.head = head;
                state.worker_epoch = fence.1;
                write(self.collection, RECEIPT, &state)?;
            }
            Ok(())
        })();
        if persistence.is_err() {
            self.bound.revoke_native_integrity();
        }
        match (work, persistence) {
            (Err(primary), Err(persist)) => {
                return Err(primary.context(format!(
                    "Verify progress persistence also failed: {persist:#}"
                )));
            }
            (Err(primary), Ok(())) if interrupted => {
                return Err(
                    match crate::service::registry::AuditDrainProof::new(
                        self.registry,
                        self.lease,
                        self.bound,
                        job,
                    ) {
                        Ok(proof) => primary.context(proof),
                        Err(_) => primary,
                    },
                );
            }
            (Err(primary), Ok(())) => return Err(primary),
            (Ok(()), Err(persist)) => return Err(persist),
            (Ok(()), Ok(())) => {}
        }
        let drained = crate::service::registry::AuditDrainProof::new(
            self.registry,
            self.lease,
            self.bound,
            job,
        )
        .ok();
        let completed = if progress.complete() && !blocked_large {
            Some(AuditCompletionProof {
                drained: crate::service::registry::AuditDrainProof::new(
                    self.registry,
                    self.lease,
                    self.bound,
                    job,
                )?,
                head: state_head_for_completion.clone(),
            })
        } else {
            None
        };
        let recovery_completed = if progress.complete() && !blocked_large {
            if let Some(epoch) = recovery {
                self.registry.require_recovery_epoch(self.lease, epoch)?;
                Some(EnabledComponentAuditProof {
                    drained: crate::service::registry::AuditDrainProof::new(
                        self.registry,
                        self.lease,
                        self.bound,
                        job,
                    )?,
                    epoch: epoch.clone(),
                    head: state_head_for_completion,
                })
            } else {
                None
            }
        } else {
            None
        };
        Ok(VerifyTurn {
            verified_files: progress.checked_files.saturating_sub(files_before),
            progress,
            blocked_large,
            drained,
            completed,
            recovery_completed,
        })
    }
}

#[cfg(test)]
pub(crate) fn test_write_recovery_receipt(
    collection: &Collection,
    value: serde_json::Value,
) -> Result<()> {
    let receipt: RecoveryReceipt = serde_json::from_value(value)?;
    receipt.validate(collection.binding)?;
    write(collection, RECOVERY_RECEIPT, &receipt)
}

#[cfg(test)]
pub(crate) fn test_recovery_receipt_bytes(value: serde_json::Value) -> Result<Vec<u8>> {
    let receipt: RecoveryReceipt = serde_json::from_value(value)?;
    Ok(serde_json::to_vec(&receipt)?)
}
/// Inert valid-format metadata for ownership tests, never a physical proof.
#[cfg(test)]
pub(crate) fn test_seed_recovery_receipt(
    collection: &Collection,
    head: &CurrentHead,
    namespace: Incarnation,
    project: airk_core::service::ProjectBinding,
) -> Result<Vec<u8>> {
    let job = Sha256Digest::from_bytes([0; 32]);
    let epoch = RecoveryContext {
        namespace,
        project,
        worker: Incarnation::from_bytes([0; 16]),
        worker_epoch: 1,
        job,
        inherited: job,
        head: head.token()?,
        root: job,
        selected: job,
        profile: NATIVE_RECOVERY_PROFILE,
    };
    let receipt = RecoveryReceipt {
        version: 1,
        owner: collection.binding,
        namespace,
        epoch,
        progress: VerifyProgress::reconcile(None, job, head, 0),
    };
    receipt.validate(collection.binding)?;
    write(collection, RECOVERY_RECEIPT, &receipt)?;
    Ok(serde_json::to_vec(&receipt)?)
}
