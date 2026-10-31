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
pub struct VerifyTurn {
    pub verified_files: u64,
    pub progress: VerifyProgress,
    pub completed: Option<AuditCompletionProof>,
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
        let old = observation.as_mut().and_then(|value| value.verify.take());
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
            state.verify = Some(progress.clone());
            state.head = head;
            state.worker_epoch = fence.1;
            write(self.collection, RECEIPT, &state)?;
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
                head: state_head_for_completion,
            })
        } else {
            None
        };
        Ok(VerifyTurn {
            verified_files: progress.checked_files.saturating_sub(files_before),
            progress,
            blocked_large,
            drained,
            completed,
        })
    }
}
