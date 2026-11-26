// SPDX-License-Identifier: Apache-2.0
//! Bounded lexical checks for the existing automatic Verify cohort.
use super::IntegrityOwner;
use crate::{
    current::{
        CurrentRead, CurrentToken, lexical, lexical_repair::MAX_CHUNKS, map::MapDomain,
        postings::Lookup,
    },
    service::registry::AutomaticAuditContext,
};
use airk_core::{
    SearchQuery,
    identity::{ArtifactId, Sha256Digest},
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::{BTreeSet, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

const FILES_PER_TURN: usize = 2;
const MAX_RECENT_PATHS: usize = 32;
const TURN_WINDOW: Duration = Duration::from_millis(250);

/// Coalesced wake result for the existing automatic Verify cohort.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LexicalAuditArm {
    Armed,
    Deferred,
    Stale,
}

/// Request-derived identity only. Independent Verify must authenticate the
/// retained chunk and controls before using it as evidence of a defect.
pub struct LexicalSearchNomination {
    head: CurrentToken,
    artifact: ArtifactId,
    source_hash: Sha256Digest,
    payload_digest: Sha256Digest,
    ordinal: u32,
    stable_id: Sha256Digest,
    policy: Option<Sha256Digest>,
}

impl LexicalSearchNomination {
    fn resolve(&self, read: &CurrentRead) -> Result<Option<String>> {
        read.require_token(&self.head)?;
        if read.source_policy_digest()? != self.policy {
            return Ok(None);
        }
        let Some(path) = read.map_get(MapDomain::ArtifactHandle, self.artifact.as_bytes())? else {
            return Ok(None);
        };
        let path = String::from_utf8(path)?;
        Ok(read.accepts_path(&path).then_some(path))
    }
}

/// Cursor stays with this live worker. A replacement starts its own sweep.
#[derive(Default)]
pub struct LexicalAuditCursor {
    head: Option<CurrentToken>,
    after: Option<String>,
    recent: BTreeSet<String>,
    search_head: Option<CurrentToken>,
    search: VecDeque<LexicalSearchNomination>,
    seen_search: BTreeSet<ArtifactId>,
    search_arm_pending: bool,
}

impl LexicalAuditCursor {
    /// Called by canonical Search only after it has a complete lexical miss
    /// with default filters. This does not store or inspect query text.
    pub fn prove_exact_search_miss(
        read: &CurrentRead,
        query: &SearchQuery,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<Option<LexicalSearchNomination>> {
        let Some(path) = query.path_prefix.as_deref().filter(|path| !path.is_empty()) else {
            return Ok(None);
        };
        checkpoint()?;
        if !read.accepts_path(path) {
            return Ok(None);
        }
        let head = read.token()?;
        let policy = read.source_policy_digest()?;
        let Some(identity) =
            read.with_index(|index| index.exact_fts_miss_identity(path, MAX_CHUNKS))?
        else {
            return Ok(None);
        };
        checkpoint()?;
        read.require_token(&head)?;
        read.require_path(path)?;
        ensure!(
            lexical::stable_id(path, u64::from(identity.ordinal))? == identity.stable_id,
            "lexical search nomination identity differs"
        );
        Ok(Some(LexicalSearchNomination {
            head,
            artifact: identity.artifact,
            source_hash: identity.source_hash,
            payload_digest: identity.payload_digest,
            ordinal: identity.ordinal,
            stable_id: identity.stable_id,
            policy,
        }))
    }

    pub fn note_search_miss(&mut self, nomination: LexicalSearchNomination) -> bool {
        if self.head.is_some_and(|head| {
            head.incarnation != nomination.head.incarnation
                || head.sequence > nomination.head.sequence
        }) || self.search_head.is_some_and(|head| {
            head.incarnation == nomination.head.incarnation
                && head.sequence > nomination.head.sequence
        }) {
            return false;
        }
        self.rebind_search(nomination.head);
        if self.seen_search.len() >= MAX_RECENT_PATHS
            || !self.seen_search.insert(nomination.artifact)
        {
            return false;
        }
        self.search.push_back(nomination);
        self.search_arm_pending = true;
        true
    }

    pub fn pending_search_arm(&self) -> Option<CurrentToken> {
        if self.search_arm_pending && !self.search.is_empty() {
            self.search_head
        } else {
            None
        }
    }

    pub fn settle_search_arm(&mut self, head: CurrentToken, disposition: LexicalAuditArm) {
        if self.search_head != Some(head) {
            return;
        }
        match disposition {
            LexicalAuditArm::Armed => self.search_arm_pending = false,
            LexicalAuditArm::Deferred => {}
            LexicalAuditArm::Stale => {
                self.search.clear();
                self.search_arm_pending = false;
            }
        }
    }

    fn rebind_search(&mut self, head: CurrentToken) {
        if self.search_head != Some(head)
            && self.search_head.is_none_or(|old| {
                old.incarnation != head.incarnation || old.sequence <= head.sequence
            })
        {
            self.search_head = Some(head);
            self.search.clear();
            self.seen_search.clear();
            self.search_arm_pending = false;
        }
    }

    fn take_search(&mut self, head: CurrentToken) -> Option<LexicalSearchNomination> {
        if self.search_head != Some(head) {
            return None;
        }
        let nomination = self.search.pop_front();
        if nomination.is_some() {
            self.search_arm_pending = !self.search.is_empty();
        }
        nomination
    }

    pub fn note_publication<'a>(
        &mut self,
        head: CurrentToken,
        changed: impl Iterator<Item = &'a str>,
    ) {
        self.rebind(head);
        for path in changed {
            if self.recent.len() >= MAX_RECENT_PATHS {
                break;
            }
            self.recent.insert(path.to_owned());
        }
    }

    fn advance(&mut self, path: String, recent: bool) {
        if recent {
            self.recent.remove(&path);
        } else {
            self.after = Some(path);
        }
    }

    fn rebind(&mut self, head: CurrentToken) {
        if self
            .head
            .is_none_or(|old| old.incarnation != head.incarnation)
        {
            self.after = None;
            self.recent.clear();
        }
        self.head = Some(head);
        self.rebind_search(head);
    }
}

#[derive(Debug)]
struct LexicalAuditYield;

impl std::fmt::Display for LexicalAuditYield {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("lexical audit turn exhausted")
    }
}
impl std::error::Error for LexicalAuditYield {}

impl IntegrityOwner<'_> {
    /// Check a short authorized page before ordinary physical Verify. Only its
    /// existing nonmanual, live cohort may publish a verified lexical recovery.
    pub fn check_automatic_lexical_turn(
        &self,
        cohort: &AutomaticAuditContext,
        cursor: &mut LexicalAuditCursor,
        authorize: impl Fn(&Arc<CurrentRead>) -> Result<()>,
        checkpoint: impl Fn() -> Result<()>,
    ) -> Result<()> {
        if cohort.manual() || self.bound.native_integrity_revoked() {
            return Ok(());
        }
        let deadline = Instant::now() + TURN_WINDOW;
        let check = || {
            checkpoint()?;
            if Instant::now() >= deadline {
                return Err(LexicalAuditYield.into());
            }
            Ok(())
        };
        let result = (|| {
            check()?;
            self.check_lexical_cohort(cohort)?;
            let selection = self.collection.select()?;
            let Some(scope) = selection.store.current_owned_scope()? else {
                return Ok(());
            };
            let read = selection.store.current_read(&scope)?;
            authorize(&read)?;
            let head = read.token()?;
            if self.native_no_damage(read.head(), false).is_err() {
                // The existing verifier owns sticky damage and recovery.
                return Ok(());
            }
            let settings = super::super::inspect_settings_digest(&selection.store, &scope.root)?;
            cursor.rebind(head);
            let nomination = cursor.take_search(head);
            let nominated_path = match &nomination {
                Some(nomination) => nomination.resolve(&read)?,
                None => None,
            };
            let nominated = nominated_path.is_some();
            let recent = !cursor.recent.is_empty();
            let (paths, next_after) = if let Some(path) = nominated_path {
                (vec![path], None)
            } else if recent {
                (
                    cursor.recent.iter().take(FILES_PER_TURN).cloned().collect(),
                    None,
                )
            } else {
                let page = read.lookup_paths_filtered_checked(
                    Lookup::Files,
                    "",
                    cursor.after.as_deref(),
                    FILES_PER_TURN,
                    |_| Ok(true),
                    check,
                )?;
                (page.paths, page.next_after)
            };
            for path in paths {
                let candidate = if read.accepts_path(&path) {
                    read.nominate_fts_restore(&head, &path, None, check)?
                } else {
                    None
                };
                let Some(candidate) = candidate else {
                    if !nominated {
                        cursor.advance(path, recent);
                    }
                    continue;
                };
                if nominated
                    && let Some(nomination) = &nomination
                    && !candidate.matches_row(
                        nomination.source_hash,
                        nomination.payload_digest,
                        nomination.ordinal,
                        nomination.stable_id,
                    )
                {
                    continue;
                }
                check()?;
                // Exclude replacement only at the actual publication boundary.
                let locked = self.collection.select_existing_locked()?;
                ensure!(
                    selection.store.directory() == locked.store.directory(),
                    "lexical audit selection changed"
                );
                let writer = locked.store.lock()?;
                ensure!(
                    super::super::settings_digest_with_writer(&locked.store, &scope.root, &writer)?
                        == settings,
                    "automatic lexical audit saved settings changed"
                );
                let permit = locked.store.capture_current_fts_permit(&writer)?;
                self.check_lexical_cohort(cohort)?;
                read.require_token(candidate.expected())?;
                let committed = candidate.with_restore(|proposal| {
                    read.require_path(proposal.missing.path)?;
                    read.require_path(proposal.positive.path)?;
                    locked
                        .store
                        .restore_current_fts(&writer, &permit, proposal, deadline, || {
                            check().is_err()
                        })
                })?;
                drop(writer);
                drop(locked);
                // Reopen a single fresh snapshot and verify using ordinary
                // lexical retrieval before treating the candidate as repaired.
                let repaired = selection.store.current_read(&scope)?;
                authorize(&repaired)?;
                repaired.require_token(&committed)?;
                let (target, literal, ordinal, stable_id) = candidate.target();
                let query = SearchQuery {
                    text: literal.to_owned(),
                    path_prefix: None,
                    limit: 1,
                    max_bytes: crate::index::FTS_RESTORE_MAX_BYTES,
                };
                let paths = BTreeSet::from([target.to_owned()]);
                let replay = repaired.lexical_search_paths(&query, Some(&paths))?;
                candidate.with_restore(|proposal| {
                    let (_, _, expected) = proposal
                        .missing
                        .expected
                        .context("repair witness missing")?;
                    ensure!(
                        !replay.incomplete
                            && matches!(replay.values.as_slice(), [hit]
                                if hit.local == u64::from(ordinal)
                                    && hit.stable_id == stable_id
                                    && serde_json::to_vec(&hit.citation)? == expected),
                        "published lexical repair did not recover its cited witness"
                    );
                    Ok(())
                })?;
                if !nominated {
                    cursor.advance(path, recent);
                }
                cursor.rebind(committed);
                self.registry
                    .retain_lexical_repair_head(self.lease, cohort, head, committed)?;
                return Ok(());
            }
            if !nominated && !recent {
                cursor.after = next_after;
            }
            Ok(())
        })();
        match result {
            Err(err) if err.is::<LexicalAuditYield>() => Ok(()),
            other => other,
        }
    }

    fn check_lexical_cohort(&self, expected: &AutomaticAuditContext) -> Result<()> {
        self.check()?;
        let live = self
            .registry
            .automatic_audit_context(self.lease, expected.job())?
            .context("lexical audit cohort retired")?;
        ensure!(
            !live.manual()
                && live.campaign() == expected.campaign()
                && live.epoch() == expected.epoch()
                && !self.registry.cancellation_requested(live.job())?,
            "lexical audit cohort changed or was cancelled"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::identity::Incarnation;

    fn ticket(sequence: u64, artifact: u8) -> LexicalSearchNomination {
        LexicalSearchNomination {
            head: CurrentToken {
                incarnation: Incarnation::from_bytes([1; 16]),
                sequence,
                digest: Sha256Digest::from_bytes([sequence as u8; 32]),
            },
            artifact: ArtifactId::from_bytes([artifact; 16]),
            source_hash: Sha256Digest::from_bytes([2; 32]),
            payload_digest: Sha256Digest::from_bytes([3; 32]),
            ordinal: 0,
            stable_id: Sha256Digest::from_bytes([4; 32]),
            policy: None,
        }
    }

    #[test]
    fn lexical_search_queue_coalesces_and_discards_stale_head() {
        let mut cursor = LexicalAuditCursor::default();
        let head = ticket(1, 0).head;
        for artifact in 0..MAX_RECENT_PATHS as u8 {
            assert!(cursor.note_search_miss(ticket(1, artifact)));
            assert!(!cursor.note_search_miss(ticket(1, artifact)));
        }
        assert!(!cursor.note_search_miss(ticket(1, 32)));
        assert_eq!(cursor.pending_search_arm(), Some(head));
        cursor.settle_search_arm(head, LexicalAuditArm::Deferred);
        assert_eq!(cursor.pending_search_arm(), Some(head));
        cursor.settle_search_arm(head, LexicalAuditArm::Armed);
        assert_eq!(cursor.pending_search_arm(), None);
        assert!(cursor.take_search(head).is_some());
        assert_eq!(cursor.pending_search_arm(), Some(head));
        assert!(!cursor.note_search_miss(ticket(1, 0)));

        let next = ticket(2, 0).head;
        assert!(cursor.note_search_miss(ticket(2, 0)));
        cursor.settle_search_arm(head, LexicalAuditArm::Stale);
        cursor.rebind(head);
        assert!(cursor.take_search(head).is_none());
        assert_eq!(cursor.pending_search_arm(), Some(next));
        cursor.settle_search_arm(next, LexicalAuditArm::Stale);
        assert_eq!(cursor.pending_search_arm(), None);
        assert!(!cursor.note_search_miss(ticket(2, 0)));
    }
}
