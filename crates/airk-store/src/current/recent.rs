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

impl CurrentRead {
    pub fn recent_changes(&self, now_unix_seconds: u64) -> Result<RecentChanges> {
        let token = self.token()?;
        let Some(bytes) = self.map_get(map::MapDomain::RecentChanges, HEADER)? else {
            return Ok(RecentChanges {
                coverage: coverage(token.sequence, CoverageReason::Unsupported),
                parents: vec![],
                events: vec![],
                view_digest: framed_digest(b"recent-change-view/1", &[b"absent"]),
            });
        };
        let mut value = manifest(&bytes, &self.head().scope, token.sequence)?;
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
        let mut events = Vec::new();
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
            events.extend(group_events(group, &mut |key| {
                self.map_get(map::MapDomain::RecentChanges, key)
            })?);
        }
        let view_digest = framed_digest(
            b"recent-change-view/1",
            &[
                &bytes,
                &first.to_be_bytes(),
                &serde_json::to_vec(&value.coverage)?,
            ],
        );
        Ok(RecentChanges {
            coverage: value.coverage,
            parents,
            events,
            view_digest,
        })
    }
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
