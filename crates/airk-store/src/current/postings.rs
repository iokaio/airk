// SPDX-License-Identifier: Apache-2.0
//! Authenticated path postings, updated only for changed artifact selectors.
//! Posting entries are candidates; retained objects remain the evidence authority.
use super::{CurrentRead, CurrentScope, journal::valid_current_path, map};
use crate::FileStore;
use airk_core::{
    Artifact,
    identity::{Sha256Digest, framed_digest},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const MAX_PATHS_PER_SHARD: usize = 4096;
const MAX_SELECTORS_PER_ARTIFACT: usize = 65_536;
const MAX_CHANGED_BYTES: usize = 64 * 1024 * 1024;
const MAX_CHANGED_KEYS: usize = 65_536;

/// Exact names include written names, qualified names and bound symbol IDs.
/// Substring queries use another selective filter or the complete file roster.
#[derive(Clone, Copy, Debug)]
pub enum Lookup<'a> {
    Files,
    FactName(&'a str),
    FactKind(&'a str),
    Language(&'a str),
    EdgeSource(&'a str),
    EdgeTarget(&'a str),
}

impl Lookup<'_> {
    fn identity(self) -> Sha256Digest {
        let (kind, value) = match self {
            Self::Files => (b"files".as_slice(), ""),
            Self::FactName(value) => (b"fact-name".as_slice(), value),
            Self::FactKind(value) => (b"fact-kind".as_slice(), value),
            Self::Language(value) => (b"language".as_slice(), value),
            Self::EdgeSource(value) => (b"edge-source".as_slice(), value),
            Self::EdgeTarget(value) => (b"edge-target".as_slice(), value),
        };
        framed_digest(b"current-posting-selector/1", &[kind, value.as_bytes()])
    }
}

fn shard(path: &str) -> u8 {
    framed_digest(b"current-posting-path/1", &[path.as_bytes()]).as_bytes()[0]
}
fn identity(selector: Sha256Digest, shard: u8) -> [u8; 33] {
    let mut key = [0; 33];
    key[..32].copy_from_slice(selector.as_bytes());
    key[32] = shard;
    key
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Posting {
    version: u32,
    selector: Sha256Digest,
    shard: u8,
    paths: BTreeSet<String>,
}
impl Posting {
    fn bytes(&self) -> Result<Vec<u8>> {
        ensure!(
            self.version == 1 && !self.paths.is_empty() && self.paths.len() <= MAX_PATHS_PER_SHARD,
            "invalid current posting count or version"
        );
        ensure!(
            self.paths
                .iter()
                .all(|path| valid_current_path(path) && shard(path) == self.shard),
            "invalid current posting path or shard"
        );
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= map::MAX_VALUE_BYTES,
            "current posting byte limit exceeded"
        );
        Ok(bytes)
    }
    fn decode(bytes: &[u8], selector: Sha256Digest, shard: u8) -> Result<Self> {
        ensure!(
            bytes.len() <= map::MAX_VALUE_BYTES,
            "oversized current posting"
        );
        let posting: Self = serde_json::from_slice(bytes)?;
        ensure!(
            posting.selector == selector && posting.shard == shard && posting.bytes()? == bytes,
            "current posting identity or encoding differs"
        );
        Ok(posting)
    }
}

fn selectors(store: &FileStore, compact: &Artifact) -> Result<BTreeSet<Sha256Digest>> {
    let artifact = store.load_retained(compact)?;
    let mut selectors = BTreeSet::from([
        Lookup::Files.identity(),
        Lookup::Language(&artifact.language).identity(),
    ]);
    for fact in &artifact.facts {
        selectors.insert(Lookup::FactName(&fact.name).identity());
        selectors.insert(Lookup::FactKind(&fact.kind).identity());
        if let Some(code) = &fact.code {
            selectors.insert(Lookup::FactName(&code.qualified_name).identity());
            selectors.insert(Lookup::FactName(&code.symbol_id).identity());
            if let Some(owner) = &code.owner_id {
                selectors.insert(Lookup::EdgeSource(owner).identity());
                selectors.insert(Lookup::EdgeTarget(&code.symbol_id).identity());
            }
            if let Some(target) = &code.target {
                let source = code
                    .owner_id
                    .clone()
                    .unwrap_or_else(|| format!("file:{}", artifact.path));
                selectors.insert(Lookup::EdgeSource(&source).identity());
                selectors.insert(Lookup::EdgeTarget(target).identity());
            }
        }
        ensure!(
            selectors.len() <= MAX_SELECTORS_PER_ARTIFACT,
            "current artifact selector limit exceeded"
        );
    }
    Ok(selectors)
}

/// Accumulates final afterimages and charges them before they can grow beyond
/// the journal's admission ceiling. No whole-corpus enumeration is performed.
#[derive(Default)]
pub(crate) struct PostingChanges {
    values: BTreeMap<Sha256Digest, map::MapChange>,
    bytes: usize,
}
impl PostingChanges {
    pub(crate) fn update(
        &mut self,
        plan: &mut map::MapPlan<'_>,
        scope: &CurrentScope,
        store: &FileStore,
        previous: Option<&Artifact>,
        next: Option<&Artifact>,
    ) -> Result<()> {
        let path = &next
            .or(previous)
            .context("posting change has no artifact")?
            .path;
        ensure!(
            valid_current_path(path) && previous.is_none_or(|old| &old.path == path),
            "posting artifact path mismatch"
        );
        let old = previous
            .map(|a| selectors(store, a))
            .transpose()?
            .unwrap_or_default();
        let new = next
            .map(|a| selectors(store, a))
            .transpose()?
            .unwrap_or_default();
        let shard = shard(path);
        for (selector, insert) in old
            .difference(&new)
            .map(|id| (*id, false))
            .chain(new.difference(&old).map(|id| (*id, true)))
        {
            let id = identity(selector, shard);
            let key = map::key(scope, map::MapDomain::Posting, &id)?;
            let mut posting = match plan.get(key)? {
                Some(bytes) => Posting::decode(&bytes, selector, shard)?,
                None => Posting {
                    version: 1,
                    selector,
                    shard,
                    paths: BTreeSet::new(),
                },
            };
            let changed = if insert {
                posting.paths.insert(path.clone())
            } else {
                posting.paths.remove(path)
            };
            ensure!(
                changed,
                "current posting differs from prior artifact membership"
            );
            let value = if posting.paths.is_empty() {
                None
            } else {
                Some(posting.bytes()?)
            };
            let old_bytes = self
                .values
                .get(&key)
                .and_then(|v| v.value())
                .map_or(0, <[u8]>::len);
            let next_bytes = self
                .bytes
                .checked_sub(old_bytes)
                .and_then(|n| n.checked_add(value.as_ref().map_or(0, Vec::len)))
                .context("current posting change byte overflow")?;
            ensure!(
                next_bytes <= MAX_CHANGED_BYTES
                    && (self.values.contains_key(&key) || self.values.len() < MAX_CHANGED_KEYS),
                "current posting transaction limit exceeded"
            );
            let change = map::MapChange::new(scope, map::MapDomain::Posting, &id, value)?;
            plan.put(key, change.value())?;
            self.bytes = next_bytes;
            self.values.insert(key, change);
        }
        Ok(())
    }
    pub(crate) fn into_changes(self) -> impl Iterator<Item = map::MapChange> {
        self.values.into_values()
    }
}

pub struct PathPage {
    pub paths: Vec<String>,
    /// Exclusive path key for the next page, only when additional paths exist.
    pub next_after: Option<String>,
}

impl CurrentRead {
    /// Authenticates all fixed shards before deciding absence or pagination.
    /// Only at most limit+1 matching paths are retained in memory. A caller's
    /// continuation must additionally bind the exact current token and filters.
    pub fn lookup_paths(
        &self,
        lookup: Lookup<'_>,
        prefix: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<PathPage> {
        ensure!(
            prefix.len() <= 8192
                && after.is_none_or(valid_current_path)
                && (1..=100_000).contains(&limit),
            "invalid current path lookup bounds"
        );
        let selector = lookup.identity();
        let mut candidates = BTreeSet::new();
        let mut bytes = 0usize;
        for shard in 0..=u8::MAX {
            let Some(encoded) =
                self.map_get(map::MapDomain::Posting, &identity(selector, shard))?
            else {
                continue;
            };
            let posting = Posting::decode(&encoded, selector, shard)?;
            for path in posting.paths {
                if path.starts_with(prefix) && after.is_none_or(|after| path.as_str() > after) {
                    bytes = bytes
                        .checked_add(path.len())
                        .context("current path page byte overflow")?;
                    candidates.insert(path);
                    if candidates.len() > limit + 1 {
                        bytes -= candidates.pop_last().expect("candidate exists").len();
                    }
                    ensure!(
                        bytes <= MAX_CHANGED_BYTES,
                        "current path page byte limit exceeded"
                    );
                }
            }
        }
        let truncated = candidates.len() > limit;
        if truncated {
            candidates.pop_last();
        }
        for path in &candidates {
            ensure!(
                self.artifact(path)?.is_some(),
                "current posting references absent artifact"
            );
        }
        let paths = candidates.into_iter().collect::<Vec<_>>();
        let next_after = truncated.then(|| paths.last().expect("nonempty page").clone());
        Ok(PathPage { paths, next_after })
    }
}
