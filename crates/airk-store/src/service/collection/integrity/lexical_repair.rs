// SPDX-License-Identifier: Apache-2.0
//! Bounded lexical checks for the existing automatic Verify cohort.
use super::IntegrityOwner;
use crate::{
    current::{CurrentRead, CurrentToken, postings::Lookup},
    service::registry::AutomaticAuditContext,
};
use airk_core::SearchQuery;
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeSet,
    sync::Arc,
    time::{Duration, Instant},
};

const FILES_PER_TURN: usize = 2;
const MAX_RECENT_PATHS: usize = 32;
const TURN_WINDOW: Duration = Duration::from_millis(250);

/// Cursor stays with this live worker. A replacement starts its own sweep.
#[derive(Default)]
pub struct LexicalAuditCursor {
    head: Option<CurrentToken>,
    after: Option<String>,
    recent: BTreeSet<String>,
}

impl LexicalAuditCursor {
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
            let recent = !cursor.recent.is_empty();
            let (paths, next_after) = if recent {
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
                    cursor.advance(path, recent);
                    continue;
                };
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
                cursor.advance(path, recent);
                cursor.head = Some(committed);
                self.registry
                    .retain_lexical_repair_head(self.lease, cohort, head, committed)?;
                return Ok(());
            }
            if !recent {
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
