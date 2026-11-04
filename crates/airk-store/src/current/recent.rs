// SPDX-License-Identifier: Apache-2.0
//! Bounded change metadata authenticated by the current map; no historical source bodies.
use super::{CurrentHead, CurrentRead, CurrentScope, CurrentToken, map};
use airk_core::{
    changes::*,
    identity::{Incarnation, RevisionToken, Sha256Digest, framed_digest},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

pub const GENERATIONS: usize = 64;
pub const AGE_SECONDS: u64 = 24 * 60 * 60;
pub const MAX_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_RECORDS: usize = 10_000;
const CHUNK_BYTES: usize = 128 * 1024;
const HEADER: &[u8] = b"manifest";

pub fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_secs())
}

#[derive(Clone, Copy, Debug)]
pub struct InvalidRecentChanges;
impl std::fmt::Display for InvalidRecentChanges {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("retained change metadata is corrupt")
    }
}
impl std::error::Error for InvalidRecentChanges {}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Group {
    generation: u64,
    parent: RevisionToken,
    observed_unix_seconds: u64,
    records: usize,
    bytes: usize,
    chunks: usize,
    digest: Sha256Digest,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    incarnation: Incarnation,
    generation: u64,
    coverage: ChangeCoverage,
    groups: Vec<Group>,
}

pub struct RecentChanges {
    pub coverage: ChangeCoverage,
    pub parents: Vec<RevisionToken>,
    pub events: Vec<ChangeEvent>,
    /// Binds logical age expiry as well as the authenticated stored manifest.
    pub view_digest: Sha256Digest,
}
impl RecentChanges {
    pub fn verifies(&self, baseline: RevisionToken) -> bool {
        baseline.generation >= self.coverage.files.complete_after
            && self.parents.contains(&baseline)
    }
}

pub(crate) struct Observation {
    pub events: Vec<ChangeEvent>,
    pub file_gap: Option<CoverageReason>,
    pub code_gap: Option<CoverageReason>,
    pub document_gap: Option<CoverageReason>,
}
impl Observation {
    pub(crate) fn empty() -> Self {
        Self {
            events: vec![],
            file_gap: None,
            code_gap: None,
            document_gap: None,
        }
    }
    pub(crate) fn omitted() -> Self {
        Self {
            file_gap: Some(CoverageReason::Omitted),
            code_gap: Some(CoverageReason::Omitted),
            document_gap: Some(CoverageReason::Omitted),
            events: vec![],
        }
    }
}

fn revision(token: CurrentToken) -> RevisionToken {
    RevisionToken {
        incarnation: token.incarnation,
        generation: token.sequence,
        revision: token.digest,
    }
}
fn chunk_key(generation: u64, chunk: usize) -> Vec<u8> {
    format!("events/{generation}/{chunk}").into_bytes()
}
fn floor(generation: u64, reason: Option<CoverageReason>) -> CoverageFloor {
    CoverageFloor {
        complete_after: generation,
        reason,
    }
}
fn coverage(generation: u64, reason: CoverageReason) -> ChangeCoverage {
    let f = floor(generation, Some(reason));
    ChangeCoverage {
        files: f.clone(),
        code: f.clone(),
        documents: f.clone(),
        ancestry: f.clone(),
        resolution: f,
    }
}
fn advance(floor: &mut CoverageFloor, generation: u64, reason: CoverageReason) {
    if generation >= floor.complete_after {
        *floor = self::floor(generation, Some(reason));
    }
}
fn expire(coverage: &mut ChangeCoverage, generation: u64, reason: CoverageReason) {
    for floor in [
        &mut coverage.files,
        &mut coverage.code,
        &mut coverage.documents,
        &mut coverage.ancestry,
        &mut coverage.resolution,
    ] {
        advance(floor, generation, reason);
    }
}

fn manifest(bytes: &[u8], scope: &CurrentScope, generation: u64) -> Result<Manifest> {
    let parsed = (|| -> Result<Manifest> {
        ensure!(bytes.len() <= 65_536, "recent manifest exceeds bound");
        let value: Manifest = serde_json::from_slice(bytes)?;
        ensure!(
            value.version == 1
                && value.incarnation == scope.incarnation
                && value.generation <= generation
                && value.groups.len() <= GENERATIONS
                && !value.groups.is_empty(),
            "invalid recent manifest scope or count"
        );
        let mut prior = None;
        let mut records = value.groups.len() + 6;
        let mut total = 0usize;
        for group in &value.groups {
            ensure!(
                group.generation > 0
                    && group.generation <= value.generation
                    && group.parent.incarnation == scope.incarnation
                    && group.parent.next_generation()? == group.generation
                    && prior.is_none_or(|g| g + 1 == group.generation),
                "invalid recent parent/order"
            );
            ensure!(
                group.bytes <= MAX_BYTES
                    && group.chunks == group.bytes.div_ceil(CHUNK_BYTES)
                    && group.records <= MAX_RECORDS,
                "invalid recent group bounds"
            );
            records = records
                .checked_add(group.records)
                .context("recent record overflow")?;
            total = total
                .checked_add(group.bytes)
                .context("recent byte overflow")?;
            prior = Some(group.generation);
        }
        ensure!(
            prior == Some(value.generation) && records <= MAX_RECORDS && total <= MAX_BYTES,
            "recent aggregate bounds exceeded"
        );
        for floor in [
            &value.coverage.files,
            &value.coverage.code,
            &value.coverage.documents,
            &value.coverage.ancestry,
            &value.coverage.resolution,
        ] {
            ensure!(
                floor.complete_after <= value.generation,
                "future recent coverage floor"
            );
        }
        ensure!(
            serde_json::to_vec(&value)? == bytes,
            "noncanonical recent manifest"
        );
        Ok(value)
    })();
    parsed.map_err(|error| error.context(InvalidRecentChanges))
}

fn group_events(
    group: &Group,
    get: &mut impl FnMut(&[u8]) -> Result<Option<Vec<u8>>>,
) -> Result<Vec<ChangeEvent>> {
    let mut bytes = Vec::with_capacity(group.bytes);
    for n in 0..group.chunks {
        let value = get(&chunk_key(group.generation, n))?
            .ok_or_else(|| anyhow::anyhow!(InvalidRecentChanges))?;
        ensure!(
            value.len() == (group.bytes - bytes.len()).min(CHUNK_BYTES),
            InvalidRecentChanges
        );
        bytes.extend(value);
    }
    (|| -> Result<Vec<ChangeEvent>> {
        ensure!(
            bytes.len() == group.bytes
                && framed_digest(b"recent-change-events/1", &[&bytes]) == group.digest,
            "recent group digest mismatch"
        );
        let events: Vec<ChangeEvent> = serde_json::from_slice(&bytes)?;
        ensure!(
            events.len() == group.records && serde_json::to_vec(&events)? == bytes,
            "recent event encoding/count mismatch"
        );
        for (n, event) in events.iter().enumerate() {
            event.validate(group.generation)?;
            ensure!(
                event.generation == group.generation && event.event_id == n as u64,
                "recent event position mismatch"
            );
        }
        Ok(events)
    })()
    .map_err(|error| error.context(InvalidRecentChanges))
}

fn scoped_event_allowed(
    scope: &airk_core::scoped_reads::CompiledReadScope,
    event: &ChangeEvent,
) -> Result<bool> {
    let mut allowed = false;
    let mut denied = false;
    let mut endpoints = 0;
    for endpoint in [&event.before, &event.after].into_iter().flatten() {
        endpoints += 1;
        if scope.allows_source(&endpoint.root, &endpoint.path) {
            allowed = true;
        } else {
            denied = true;
        }
    }
    ensure!(
        endpoints > 0 && !(allowed && denied),
        airk_core::scoped_reads::ScopeReadRefused
    );
    Ok(allowed)
}

impl CurrentRead {
    fn scope_recent_changes(&self, mut view: RecentChanges) -> Result<RecentChanges> {
        let Some(scope) = self.read_scope.get() else {
            return Ok(view);
        };
        self.require_read_scope_live()?;
        // Shared authenticated change pages are metadata, selected before the
        // caller applies response record/count budgets or loads source bodies.
        let mut selected = Vec::new();
        for event in view.events {
            if scoped_event_allowed(scope, &event)? {
                selected.push(event);
            }
        }
        view.events = selected;
        view.view_digest = framed_digest(
            b"scoped-recent-change-view/1",
            &[view.view_digest.as_bytes(), scope.fingerprint.as_bytes()],
        );
        Ok(view)
    }

    /// Admission for the complete retained recent component, independent of age.
    /// Expired stored groups still consume bytes until publication retires them.
    pub(crate) fn recent_audit_bytes(&self) -> Result<u64> {
        let Some(bytes) = self.map_get(map::MapDomain::RecentChanges, HEADER)? else {
            return Ok(0);
        };
        let value = manifest(&bytes, &self.head().scope, self.token()?.sequence)?;
        Ok(bytes.len() as u64
            + value
                .groups
                .iter()
                .map(|group| group.bytes as u64)
                .sum::<u64>())
    }
    /// Physical maintenance covers every retained group, including groups a
    /// selected query or wall-clock expiry would omit. Call only after admission.
    pub(crate) fn audit_recent_changes(&self) -> Result<()> {
        let Some(bytes) = self.map_get(map::MapDomain::RecentChanges, HEADER)? else {
            return Ok(());
        };
        let value = manifest(&bytes, &self.head().scope, self.token()?.sequence)?;
        for group in &value.groups {
            group_events(group, &mut |key| {
                self.map_get(map::MapDomain::RecentChanges, key)
            })?;
        }
        self.token()?;
        Ok(())
    }
    /// Select only groups after an actually verified baseline. The sealed owner
    /// profile requires complete recent maintenance; header/age identity remains
    /// complete and unchanged even when no event pages need materialization.
    pub fn recent_changes_since(
        &self,
        now_unix_seconds: u64,
        baseline: Option<RevisionToken>,
    ) -> Result<RecentChanges> {
        if !self.private_query_eligible()? {
            return self.recent_changes(now_unix_seconds);
        }
        let token = self.token()?;
        let current = revision(token);
        let result = read_recent(
            &self.head().scope,
            token,
            now_unix_seconds,
            |view| {
                baseline
                    .filter(|baseline| *baseline != current && view.verifies(*baseline))
                    .map(|baseline| baseline.generation)
            },
            &mut |key| self.map_get(map::MapDomain::RecentChanges, key),
        );
        let result = result.map_err(|error| {
            if error.downcast_ref::<InvalidRecentChanges>().is_some() {
                error.context(super::PrivateIntegrityFailure)
            } else {
                error
            }
        });
        let view = self.scope_recent_changes(self.observe_private_integrity(result)?)?;
        self.token()?;
        Ok(view)
    }
    pub fn recent_changes(&self, now_unix_seconds: u64) -> Result<RecentChanges> {
        // Ordinary queries/diagnostics retain full unexpired-group verification.
        // Selection is private preparation, not a caller-granted read authority.
        let view = read_recent(
            &self.head().scope,
            self.token()?,
            now_unix_seconds,
            |_| Some(0),
            &mut |key| self.map_get(map::MapDomain::RecentChanges, key),
        )?;
        let view = self.scope_recent_changes(view)?;
        self.token()?;
        Ok(view)
    }
}

/// Internal preparation engine. The public reader ALWAYS selects all groups.
/// A sealed native owner binds the exact enabled recent-component
/// policy/maintenance contract before exposing selected materialization. This
/// crate-internal closure is not a public bypass or a serialized capability.
pub(super) fn read_recent(
    scope: &CurrentScope,
    token: CurrentToken,
    now_unix_seconds: u64,
    select_after: impl FnOnce(&RecentChanges) -> Option<u64>,
    get: &mut impl FnMut(&[u8]) -> Result<Option<Vec<u8>>>,
) -> Result<RecentChanges> {
    let Some(bytes) = get(HEADER)? else {
        return Ok(RecentChanges {
            coverage: coverage(token.sequence, CoverageReason::Unsupported),
            parents: vec![],
            events: vec![],
            view_digest: framed_digest(b"recent-change-view/1", &[b"absent"]),
        });
    };
    let mut value = manifest(&bytes, scope, token.sequence)?;
    if value.generation != token.sequence {
        return Ok(RecentChanges {
            coverage: coverage(token.sequence, CoverageReason::Omitted),
            parents: vec![],
            events: vec![],
            view_digest: framed_digest(b"recent-change-view/1", &[&bytes, b"publication-gap"]),
        });
    }
    let expired_through = value
        .groups
        .iter()
        .filter(|g| now_unix_seconds.saturating_sub(g.observed_unix_seconds) >= AGE_SECONDS)
        .map(|g| g.generation)
        .max();
    let mut parents = Vec::new();
    let mut first = token.sequence;
    for group in &value.groups {
        if expired_through.is_some_and(|last| group.generation <= last) {
            expire(
                &mut value.coverage,
                group.generation,
                CoverageReason::Expired,
            );
            continue;
        }
        if parents.is_empty() {
            first = group.parent.generation;
        }
        parents.push(group.parent);
    }
    let view_digest = framed_digest(
        b"recent-change-view/1",
        &[
            &bytes,
            &first.to_be_bytes(),
            &serde_json::to_vec(&value.coverage)?,
        ],
    );
    let mut view = RecentChanges {
        coverage: value.coverage,
        parents,
        events: vec![],
        view_digest,
    };
    // Header identity, full independent coverage floors and age are established
    // BEFORE deciding whether a verified baseline needs event materialization.
    // Never compute a selected-only view digest or cache the wall-clock result.
    if let Some(after) = select_after(&view) {
        for group in &value.groups {
            if !expired_through.is_some_and(|last| group.generation <= last)
                && group.generation > after
            {
                view.events.extend(group_events(group, get)?);
            }
        }
    }
    Ok(view)
}

/// All keys/values enter the same bounded journal and SQL publication as the
/// artifact head. The current token is computed afterward; only older tokens
/// are embedded here, avoiding a root/token cycle.
pub(crate) fn prepare(
    scope: &CurrentScope,
    plan: &mut map::MapPlan<'_>,
    base: &CurrentHead,
    mut observation: Observation,
    now: u64,
) -> Result<Vec<map::MapChange>> {
    let generation = base
        .sequence
        .checked_add(1)
        .context("recent generation overflow")?;
    let previous = plan
        .get(map::key(scope, map::MapDomain::RecentChanges, HEADER)?)?
        .map(|bytes| manifest(&bytes, scope, base.sequence))
        .transpose()?;
    let mut value = previous.unwrap_or(Manifest {
        version: 1,
        incarnation: scope.incarnation,
        generation: base.sequence,
        coverage: coverage(base.sequence, CoverageReason::BaselineUnavailable),
        groups: vec![],
    });
    let mut changes = Vec::new();
    if value.generation != base.sequence {
        for group in value.groups.drain(..) {
            remove(scope, plan, &group, &mut changes)?;
        }
        value.coverage = coverage(base.sequence, CoverageReason::Omitted);
    }
    // Validate retained data before extending its authenticated chain.
    for group in &value.groups {
        group_events(group, &mut |key| {
            plan.get(map::key(scope, map::MapDomain::RecentChanges, key)?)
        })?;
    }
    for (n, event) in observation.events.iter_mut().enumerate() {
        ensure!(
            event.generation == generation,
            "foreign recent event generation"
        );
        event.event_id = n as u64;
        event.validate(generation)?;
    }
    let mut encoded = serde_json::to_vec(&observation.events)?;
    if observation.events.len() + 7 > MAX_RECORDS || encoded.len() > MAX_BYTES {
        observation.events.clear();
        encoded = b"[]".to_vec();
        observation.file_gap = Some(CoverageReason::Overflow);
        observation.code_gap = Some(CoverageReason::Overflow);
        observation.document_gap = Some(CoverageReason::Overflow);
    }
    for (gap, floor) in [
        (observation.file_gap, &mut value.coverage.files),
        (observation.code_gap, &mut value.coverage.code),
        (observation.document_gap, &mut value.coverage.documents),
    ] {
        if let Some(reason) = gap {
            advance(floor, generation, reason);
        }
    }
    value.coverage.resolution = floor(generation, Some(CoverageReason::Unsupported));
    let group = Group {
        generation,
        parent: revision(base.token()?),
        observed_unix_seconds: now,
        records: observation.events.len(),
        bytes: encoded.len(),
        chunks: encoded.len().div_ceil(CHUNK_BYTES),
        digest: framed_digest(b"recent-change-events/1", &[&encoded]),
    };
    for (n, bytes) in encoded.chunks(CHUNK_BYTES).enumerate() {
        put(
            scope,
            plan,
            &chunk_key(generation, n),
            Some(bytes.to_vec()),
            &mut changes,
        )?;
    }
    value.groups.push(group);
    while value.groups.len() > GENERATIONS
        || value.groups.iter().map(|g| g.records + 1).sum::<usize>() + 6 > MAX_RECORDS
        || value.groups.iter().map(|g| g.bytes).sum::<usize>() > MAX_BYTES
        || value
            .groups
            .iter()
            .any(|g| now.saturating_sub(g.observed_unix_seconds) >= AGE_SECONDS)
    {
        let age_expiry = value
            .groups
            .iter()
            .any(|g| now.saturating_sub(g.observed_unix_seconds) >= AGE_SECONDS);
        let old = value.groups.remove(0);
        let reason = if age_expiry {
            CoverageReason::Expired
        } else {
            CoverageReason::Overflow
        };
        expire(&mut value.coverage, old.generation, reason);
        remove(scope, plan, &old, &mut changes)?;
    }
    value.generation = generation;
    value.coverage.ancestry = floor(
        value
            .groups
            .first()
            .map_or(generation, |g| g.parent.generation),
        None,
    );
    let bytes = serde_json::to_vec(&value)?;
    manifest(&bytes, scope, generation)?;
    put(scope, plan, HEADER, Some(bytes), &mut changes)?;
    Ok(changes)
}

fn put(
    scope: &CurrentScope,
    plan: &mut map::MapPlan<'_>,
    key: &[u8],
    value: Option<Vec<u8>>,
    changes: &mut Vec<map::MapChange>,
) -> Result<()> {
    let change = map::MapChange::new(scope, map::MapDomain::RecentChanges, key, value)?;
    plan.put(change.key(), change.value())?;
    changes.push(change);
    Ok(())
}
fn remove(
    scope: &CurrentScope,
    plan: &mut map::MapPlan<'_>,
    group: &Group,
    changes: &mut Vec<map::MapChange>,
) -> Result<()> {
    for n in 0..group.chunks {
        put(scope, plan, &chunk_key(group.generation, n), None, changes)?;
    }
    Ok(())
}

#[cfg(test)]
mod scoped_changes_tests {
    use super::*;
    use airk_core::{
        identity::ArtifactId,
        index_catalog::PathScope,
        scoped_reads::{CompiledReadScope, CompiledSourceAtom, ScopeReadRefused},
    };

    fn endpoint(path: &str) -> ChangeEndpoint {
        ChangeEndpoint {
            citation_path: None,
            root: "original-root".into(),
            path: path.into(),
            source_hash: Sha256Digest::from_bytes([1; 32]),
            profile_id: Sha256Digest::from_bytes([2; 32]),
        }
    }
    fn event(before: &str, after: &str) -> ChangeEvent {
        ChangeEvent {
            generation: 1,
            event_id: 0,
            subject: ChangeSubject::Artifact {
                artifact_id: ArtifactId::from_bytes([3; 16]),
            },
            before: Some(endpoint(before)),
            after: Some(endpoint(after)),
            before_detail: None,
            after_detail: None,
            dimensions: vec![ChangeDimension::Path],
            cause: ChangeCause::Modified,
            basis: ContinuityBasis::UniqueCompatibleContent,
        }
    }
    #[test]
    fn moves_across_scope_are_refused_in_both_directions() -> Result<()> {
        let scope = CompiledReadScope {
            fingerprint: Sha256Digest::from_bytes([4; 32]),
            atoms: vec![],
            external_aliases: vec![],
            source_atoms: vec![CompiledSourceAtom {
                original_root: "original-root".into(),
                kind: PathScope::Subtree,
                path: "public".into(),
            }],
        };
        for (before, after) in [("public/a", "private/a"), ("private/a", "public/a")] {
            assert!(
                scoped_event_allowed(&scope, &event(before, after))
                    .unwrap_err()
                    .downcast_ref::<ScopeReadRefused>()
                    .is_some()
            );
        }
        assert!(scoped_event_allowed(
            &scope,
            &event("public/a", "public/b")
        )?);
        assert!(!scoped_event_allowed(
            &scope,
            &event("private/a", "private/b")
        )?);
        let mut pathless = event("public/a", "public/b");
        pathless.before = None;
        pathless.after = None;
        assert!(
            scoped_event_allowed(&scope, &pathless)
                .unwrap_err()
                .downcast_ref::<ScopeReadRefused>()
                .is_some()
        );
        Ok(())
    }
}
