// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::collections::{BTreeMap, BTreeSet};
const FILE: &str = "usage.airk";
const KIND: &str = "airk-usage/2";
const MAXIMUM: usize = 8 << 20;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Seen {
    sequence: u64,
    timestamp: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    version: u32,
    scope: UsageScope,
    incarnation: String,
    sequence: u64,
    supplemental_sequence: u64,
    daily: BTreeMap<u64, UsageReport>,
    lifetime: Option<UsageReport>,
    seen: BTreeMap<String, Seen>,
    replay_floor: Option<u64>,
    duplicates: u64,
    diagnostics: BTreeMap<String, UsageObservation>,
    feedback: BTreeMap<String, Usefulness>,
    supplemental_daily: BTreeMap<u64, SupplementalBucket>,
    supplemental_seen: BTreeMap<String, Seen>,
    supplemental_replay_floor: Option<u64>,
    supplemental_duplicates: u64,
    session_links: BTreeSet<String>,
}
impl State {
    fn new(scope: UsageScope) -> Result<Self> {
        let mut random = [0; 16];
        getrandom::fill(&mut random)
            .map_err(|error| anyhow::anyhow!("usage identity allocation failed: {error}"))?;
        Ok(Self {
            version: 2,
            scope,
            incarnation: hex(&random),
            sequence: 0,
            supplemental_sequence: 0,
            daily: BTreeMap::new(),
            lifetime: None,
            seen: BTreeMap::new(),
            replay_floor: None,
            duplicates: 0,
            diagnostics: BTreeMap::new(),
            feedback: BTreeMap::new(),
            supplemental_daily: BTreeMap::new(),
            supplemental_seen: BTreeMap::new(),
            supplemental_replay_floor: None,
            supplemental_duplicates: 0,
            session_links: BTreeSet::new(),
        })
    }
    fn validate(&self, scope: &UsageScope) -> Result<()> {
        ensure!(
            self.version == 2 && self.scope == *scope && valid_id(&self.incarnation),
            "usage history belongs to another root binding or format"
        );
        ensure!(
            self.seen.len() <= MAX_DEDUP
                && self.supplemental_seen.len() <= 256
                && self.diagnostics.len() <= MAX_DIAGNOSTICS
                && self.daily.len() <= 30
                && self.supplemental_daily.len() <= 30
                && self.feedback.len() <= MAX_DEDUP
                && self.session_links.len() <= MAX_DEDUP,
            "usage ledger count bound exceeded"
        );
        for (day, bucket) in &self.daily {
            ensure!(
                *day < u64::MAX / DAY_MS
                    && bucket.scope == *scope
                    && bucket.start_unix_ms == day * DAY_MS,
                "invalid usage daily binding"
            );
            bounded(bucket)?;
        }
        if let Some(bucket) = &self.lifetime {
            ensure!(bucket.scope == *scope, "lifetime scope mismatch");
            bounded(bucket)?;
        }
        for (day, bucket) in &self.supplemental_daily {
            ensure!(
                *day == bucket.day && bucket.summaries.len() <= 16 && *day < u64::MAX / DAY_MS,
                "invalid supplemental bucket"
            );
            bounded(bucket)?;
        }
        ensure!(
            self.seen
                .keys()
                .chain(self.supplemental_seen.keys())
                .all(|id| valid_id(id)),
            "invalid stored usage identifier"
        );
        ensure!(
            self.feedback
                .keys()
                .chain(self.session_links.iter())
                .all(|id| self.seen.contains_key(id)),
            "dangling usage feedback or session link"
        );
        Ok(())
    }
    fn prune(&mut self, now: u64, policy: &UsagePolicy) {
        let oldest = (now / DAY_MS).saturating_sub(u64::from(policy.aggregate_retention_days) - 1);
        self.daily.retain(|day, _| *day >= oldest);
        self.supplemental_daily.retain(|day, _| *day >= oldest);
        self.seen
            .retain(|_, seen| seen.timestamp >= oldest * DAY_MS);
        self.supplemental_seen
            .retain(|_, seen| seen.timestamp >= oldest * DAY_MS);
        self.feedback.retain(|id, _| self.seen.contains_key(id));
        self.session_links.retain(|id| self.seen.contains_key(id));
        if policy.mode == RecordingMode::DiagnosticEvents {
            let floor =
                now.saturating_sub(u64::from(policy.diagnostic_retention_hours) * 3_600_000);
            self.diagnostics.retain(|_, event| event.unix_ms >= floor);
        } else {
            self.diagnostics.clear();
        }
    }
}
fn valid_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|value| format!("{value:02x}")).collect()
}
fn bounded<T: Serialize>(value: &T) -> Result<()> {
    ensure!(
        serde_json::to_vec(value)?.len() <= MAX_BUCKET_BYTES,
        "usage aggregate dimensions exceed bucket cap"
    );
    Ok(())
}
fn retention(timestamp: u64, now: u64, policy: &UsagePolicy) -> Result<()> {
    let oldest = (now / DAY_MS).saturating_sub(u64::from(policy.aggregate_retention_days) - 1);
    ensure!(
        timestamp <= now.saturating_add(60_000) && timestamp / DAY_MS >= oldest,
        "usage timestamp outside retention window"
    );
    Ok(())
}
fn evict(seen: &mut BTreeMap<String, Seen>, maximum: usize, floor: &mut Option<u64>) {
    while seen.len() > maximum {
        if let Some(id) = seen
            .iter()
            .min_by_key(|(_, seen)| seen.sequence)
            .map(|(id, _)| id.clone())
        {
            if let Some(evicted) = seen.remove(&id) {
                *floor = Some(floor.unwrap_or(0).max(evicted.timestamp));
            }
        } else {
            break;
        }
    }
}
fn read_state(directory: &Path, scope: &UsageScope) -> Result<State> {
    let state: State = crate::ledger::read(&directory.join(FILE), KIND, 1 << 20, MAXIMUM)?;
    state.validate(scope)?;
    Ok(state)
}

#[derive(Clone)]
pub struct UsageStore {
    directory: PathBuf,
    scope: UsageScope,
    incarnation: String,
    best_effort: bool,
}
impl UsageRecorder for UsageStore {
    fn record(&self, event: &UsageObservation) -> Result<()> {
        Self::record(self, event)
    }
}
impl UsageStore {
    pub fn open(
        directory: &Path,
        scope: &UsageScope,
        policy: &UsagePolicy,
    ) -> Result<Option<Self>> {
        Self::open_mode(directory, scope, policy, false)
    }
    pub fn open_best_effort(
        directory: &Path,
        scope: &UsageScope,
        policy: &UsagePolicy,
    ) -> Result<Option<Self>> {
        Self::open_mode(directory, scope, policy, true)
    }
    fn open_mode(
        directory: &Path,
        scope: &UsageScope,
        requested: &UsagePolicy,
        best_effort: bool,
    ) -> Result<Option<Self>> {
        if requested.mode == RecordingMode::Disabled {
            return Ok(None);
        }
        validate_policy(requested)?;
        let _guard = operation_guard(directory)?;
        managed(directory)?;
        crate::validate_collection(&scope.collection)?;
        let state = if directory.join(FILE).try_exists()? {
            read_state(directory, scope)?
        } else {
            ensure!(
                !directory.join("usage.sqlite3").try_exists()?,
                "existing usage history requires an authenticated upgrade import; old ledger preserved"
            );
            let root = directory
                .parent()
                .and_then(Path::parent)
                .context("invalid collection directory")?;
            let _quota_guard = lock_with_budget(root.join("usage-quota.lock"), CONTENTION_BUDGET)?;
            let mut count = 1u64;
            let mut visited = 0usize;
            for namespace in ["collections-1.2", "collections-1.4"] {
                let path = root.join(namespace);
                if !path.try_exists()? {
                    continue;
                }
                for entry in fs::read_dir(path)? {
                    let entry = entry?;
                    visited += 1;
                    ensure!(visited <= 4096, "usage collection inventory exceeds bound");
                    if entry.file_type()?.is_dir()
                        && (entry.path().join(FILE).try_exists()?
                            || entry.path().join("usage.sqlite3").try_exists()?)
                    {
                        count += 1;
                    }
                }
            }
            ensure!(
                count.saturating_mul(1 << 20) <= requested.max_data_root_bytes,
                "usage data-root capacity reached"
            );
            let _quota = reserve_ledger(directory, requested, best_effort)?;
            let state = State::new(scope.clone())?;
            crate::ledger::create(
                &directory.join(FILE),
                KIND,
                requested.max_collection_bytes,
                MAXIMUM,
                state,
            )?;
            read_state(directory, scope)?
        };
        Ok(Some(Self {
            directory: directory.into(),
            scope: scope.clone(),
            incarnation: state.incarnation,
            best_effort,
        }))
    }
    fn write<R>(
        &self,
        policy: &UsagePolicy,
        operation: impl FnOnce(&mut State) -> Result<R>,
    ) -> Result<R> {
        let _quota = reserve_ledger(&self.directory, policy, self.best_effort)?;
        crate::ledger::write(
            &self.directory.join(FILE),
            KIND,
            policy.max_collection_bytes,
            MAXIMUM,
            |state: &mut State| {
                state.validate(&self.scope)?;
                ensure!(
                    state.incarnation == self.incarnation,
                    "usage history was cleared or collection recreated"
                );
                let result = operation(state)?;
                state.validate(&self.scope)?;
                Ok(result)
            },
        )
    }
    pub fn record(&self, event: &UsageObservation) -> Result<()> {
        ensure!(
            event.scope == self.scope,
            "usage observation scope mismatch"
        );
        let _guard = operation_guard(&self.directory)?;
        managed(&self.directory)?;
        let policy = policy(&self.directory)?;
        if policy.mode == RecordingMode::Disabled {
            return Ok(());
        }
        let now = unix_ms();
        retention(event.unix_ms, now, &policy)?;
        self.write(&policy, |state| {
            state.prune(now, &policy);
            let id = hex(&event.event_id);
            if state.seen.contains_key(&id) { state.duplicates = state.duplicates.checked_add(1).context("usage duplicate counter overflow")?; return Ok(()); }
            ensure!(state.replay_floor.is_none_or(|floor| event.unix_ms > floor), "observation older than bounded replay protection; dropped rather than counted twice");
            let day = event.unix_ms / DAY_MS;
            let mut aggregate = summarize(&self.scope, day * DAY_MS, (day + 1) * DAY_MS, [event], ByteTokenHeuristic::default())?;
            let mut lifetime = state.lifetime.take().unwrap_or(summarize(&self.scope, day * DAY_MS, (day + 1) * DAY_MS, [], ByteTokenHeuristic::default())?);
            lifetime.start_unix_ms = lifetime.start_unix_ms.min(day * DAY_MS);
            lifetime.end_unix_ms_exclusive = lifetime.end_unix_ms_exclusive.max((day + 1) * DAY_MS);
            lifetime.merge(&aggregate)?;
            bounded(&lifetime)?;
            state.lifetime = Some(lifetime);
            if let Some(old) = state.daily.get(&day) { aggregate.merge(old)?; }
            bounded(&aggregate)?;
            state.daily.insert(day, aggregate);
            state.sequence = state.sequence.checked_add(1).context("usage sequence overflow")?;
            state.seen.insert(id.clone(), Seen { sequence: state.sequence, timestamp: event.unix_ms });
            evict(&mut state.seen, MAX_DEDUP, &mut state.replay_floor);
            state.feedback.retain(|id,_| state.seen.contains_key(id));
            state.session_links.retain(|id| state.seen.contains_key(id));
            if policy.mode == RecordingMode::DiagnosticEvents {
                ensure!(serde_json::to_vec(event)?.len() <= 4096, "diagnostic observation too large");
                state.diagnostics.insert(id, event.clone());
                while state.diagnostics.len() > MAX_DIAGNOSTICS {
                    let oldest = state.diagnostics.iter().min_by_key(|(_,event)| event.unix_ms).map(|(id,_)| id.clone()).context("diagnostic order missing")?;
                    state.diagnostics.remove(&oldest);
                }
            }
            Ok(())
        })
    }
    pub fn read(directory: &Path, scope: &UsageScope, start: u64, end: u64) -> Result<StoredUsage> {
        let mut result = StoredUsage {
            buckets: Vec::new(),
            lifetime: None,
            supplemental: Vec::new(),
            health: RecordingHealth {
                deduplication_capacity: MAX_DEDUP,
                ..Default::default()
            },
        };
        if !directory.join(FILE).try_exists()? {
            ensure!(
                !directory.join("usage.sqlite3").try_exists()?,
                "existing usage history requires upgrade import; old ledger preserved"
            );
            return Ok(result);
        }
        let _guard = operation_guard(directory)?;
        managed(directory)?;
        let state = read_state(directory, scope)?;
        let policy = policy(directory)?;
        let oldest = (unix_ms() / DAY_MS)
            .saturating_sub(u64::from(policy.aggregate_retention_days) - 1)
            * DAY_MS;
        let start = start.max(oldest);
        i64::try_from(start)?;
        i64::try_from(end)?;
        result.buckets = state
            .daily
            .into_iter()
            .filter(|(day, _)| *day * DAY_MS >= start && *day * DAY_MS < end)
            .map(|(_, value)| value)
            .collect();
        result.supplemental = state
            .supplemental_daily
            .into_iter()
            .filter(|(day, _)| *day * DAY_MS >= start && *day * DAY_MS < end)
            .map(|(_, value)| value)
            .collect();
        result.lifetime = state.lifetime;
        result.health.duplicate_observations = state.duplicates;
        result.health.supplemental_duplicate_observations = state.supplemental_duplicates;
        result.health.database_bytes = fs::metadata(directory.join(FILE))?.len();
        let days: BTreeSet<_> = result
            .buckets
            .iter()
            .map(|b| b.start_unix_ms / DAY_MS)
            .chain(result.supplemental.iter().map(|b| b.day))
            .collect();
        result.health.retained_days = days.len();
        result.health.earliest_retained_unix_ms = days.first().map(|day| day * DAY_MS);
        result.health.latest_retained_unix_ms = days.last().map(|day| (day + 1) * DAY_MS);
        for (id, usefulness) in state.feedback {
            if state
                .seen
                .get(&id)
                .is_some_and(|seen| seen.timestamp >= start && seen.timestamp < end)
            {
                match usefulness {
                    Usefulness::Useful => result.health.useful_feedback += 1,
                    Usefulness::PartlyUseful => result.health.partly_useful_feedback += 1,
                    Usefulness::NotUseful => result.health.not_useful_feedback += 1,
                }
            }
        }
        Ok(result)
    }
    pub fn record_supplement(&self, event: &SupplementalMeasurement) -> Result<bool> {
        event.validate()?;
        let _guard = operation_guard(&self.directory)?;
        managed(&self.directory)?;
        let policy = policy(&self.directory)?;
        ensure!(
            policy.mode != RecordingMode::Disabled,
            "usage recording is disabled"
        );
        let now = unix_ms();
        retention(event.unix_ms, now, &policy)?;
        self.write(&policy, |state| {
            state.prune(now, &policy);
            let id = event.event_id.to_ascii_lowercase();
            if state.supplemental_seen.contains_key(&id) {
                state.supplemental_duplicates = state
                    .supplemental_duplicates
                    .checked_add(1)
                    .context("supplemental duplicate overflow")?;
                return Ok(false);
            }
            ensure!(
                state
                    .supplemental_replay_floor
                    .is_none_or(|floor| event.unix_ms > floor),
                "supplemental observation older than bounded replay protection"
            );
            for request in &event.airk_request_ids {
                let request = request.to_ascii_lowercase();
                ensure!(
                    state.seen.contains_key(&request),
                    "linked AIRK request is unknown, expired or belongs to another collection"
                );
                ensure!(
                    state.session_links.insert(request),
                    "AIRK request already linked to another session import"
                );
            }
            let day = event.unix_ms / DAY_MS;
            let bucket =
                state
                    .supplemental_daily
                    .entry(day)
                    .or_insert_with(|| SupplementalBucket {
                        day,
                        summaries: Vec::new(),
                    });
            let mut new = SupplementSummary::new(event);
            if let Some(old) = bucket
                .summaries
                .iter_mut()
                .find(|summary| summary.stratum == new.stratum)
            {
                old.observe(event)?;
            } else {
                ensure!(
                    bucket.summaries.len() < 16,
                    "supplemental dimensions reached daily cap of 16; input was not recorded"
                );
                new.observe(event)?;
                bucket.summaries.push(new);
            }
            bounded(bucket)?;
            state.supplemental_sequence = state
                .supplemental_sequence
                .checked_add(1)
                .context("supplemental sequence overflow")?;
            state.supplemental_seen.insert(
                id,
                Seen {
                    sequence: state.supplemental_sequence,
                    timestamp: event.unix_ms,
                },
            );
            evict(
                &mut state.supplemental_seen,
                256,
                &mut state.supplemental_replay_floor,
            );
            Ok(true)
        })
    }
}

pub fn feedback(
    directory: &Path,
    scope: &UsageScope,
    request_id: &str,
    usefulness: Usefulness,
) -> Result<()> {
    ensure!(
        request_id.len() == 32 && request_id.bytes().all(|value| value.is_ascii_hexdigit()),
        "request ID must be 32 hex digits"
    );
    let _guard = operation_guard(directory)?;
    managed(directory)?;
    let policy = policy(directory)?;
    ensure!(
        policy.mode != RecordingMode::Disabled,
        "usage recording is disabled"
    );
    let _quota = reserve_ledger(directory, &policy, false)?;
    crate::ledger::write(
        &directory.join(FILE),
        KIND,
        policy.max_collection_bytes,
        MAXIMUM,
        |state: &mut State| {
            state.validate(scope)?;
            let id = request_id.to_ascii_lowercase();
            ensure!(
                state.seen.contains_key(&id),
                "request ID is unknown or outside retained feedback window"
            );
            state.feedback.insert(id, usefulness);
            Ok(())
        },
    )
}
pub fn clear(directory: &Path) -> Result<()> {
    let _guard = operation_guard(directory)?;
    managed(directory)?;
    for name in [FILE, "usage.sqlite3", "usage.sqlite3-journal"] {
        match fs::remove_file(directory.join(name)) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    crate::maintenance::sync_parent(directory)
}
