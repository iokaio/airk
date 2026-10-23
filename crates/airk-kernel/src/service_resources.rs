// SPDX-License-Identifier: Apache-2.0
//! Finite resident/work admission with fair project rotation and retained charges.
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::{JobKind, ProjectBinding, WorkerIdentity},
    service_resources::{ResourceConfiguration, ResourcePreset, ResourceSnapshot, WorkAllowance},
};
use anyhow::{Result, ensure};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

// Include the resident baseline and its one admitted bounded query materialization.
const RESIDENT_BYTES: u64 = 512 << 20;
const MODEL_BYTES: u64 = 512 << 20;
// Main service baseline plus the bounded, independently killable source watcher.
const SERVICE_BYTES: u64 = (32 + 128) << 20;
const WORK_BYTES: u64 = 768 << 20;
const EXTRA_EXTRACTOR_BYTES: u64 = (2 * airk_ingest::ExtractionExecution::MAX_RESULT_BYTES
    + airk_ingest::ExtractionExecution::MAX_PARALLEL_SOURCE_BYTES
    + (64 << 20)) as u64;
struct Waiting {
    job: Sha256Digest,
    kind: JobKind,
    first_seen: Instant,
    sequence: u64,
}
pub struct Resources {
    configuration: ResourceConfiguration,
    cpus: usize,
    memory: Option<u64>,
    limit: u64,
    source_threads: u16,
    inference_threads: u16,
    resident_limit: usize,
    residents: BTreeMap<ProjectBinding, WorkerIdentity>,
    running: BTreeMap<ProjectBinding, WorkAllowance>,
    waiting: BTreeMap<ProjectBinding, Waiting>,
    sequence: u64,
    last_project: Option<ProjectBinding>,
    rss: Option<u64>,
    available: Option<u64>,
    complete: bool,
    blocked: bool,
}
impl Resources {
    pub fn detect(configuration: ResourceConfiguration) -> Result<Self> {
        Self::new(
            configuration,
            std::thread::available_parallelism().map_or(1, usize::from),
            crate::limits::machine::memory_bytes(),
        )
    }
    pub fn new(
        configuration: ResourceConfiguration,
        cpus: usize,
        memory: Option<u64>,
    ) -> Result<Self> {
        configuration.validate()?;
        let cpus = cpus.clamp(1, 256);
        let numerator = match configuration.preset {
            ResourcePreset::Responsive => 1,
            ResourcePreset::Balanced => 2,
            ResourcePreset::Throughput => 3,
        };
        let ceiling = if configuration.preset == ResourcePreset::Throughput {
            8 << 30
        } else {
            4 << 30
        };
        let limit = configuration
            .memory_bytes
            .unwrap_or((memory.unwrap_or(2 << 30) / 4 * numerator).clamp(2 << 30, ceiling))
            .min(memory.filter(|bytes| *bytes > 0).unwrap_or(u64::MAX));
        let inference_threads = configuration
            .inference_threads
            .unwrap_or(1)
            .min(cpus as u16);
        let source_default = match configuration.preset {
            ResourcePreset::Responsive => 1,
            ResourcePreset::Balanced => (cpus / 2).max(1),
            ResourcePreset::Throughput => cpus.saturating_sub(2).max(1),
        };
        let source_threads = configuration
            .extraction_threads
            .unwrap_or(source_default.min(16) as u16)
            .min(cpus as u16);
        let resident_limit = ((limit.saturating_sub(MODEL_BYTES + SERVICE_BYTES + WORK_BYTES))
            / RESIDENT_BYTES)
            .clamp(1, 32) as usize;
        Ok(Self {
            configuration,
            cpus,
            memory,
            limit,
            source_threads,
            inference_threads,
            resident_limit,
            residents: BTreeMap::new(),
            running: BTreeMap::new(),
            waiting: BTreeMap::new(),
            sequence: 0,
            last_project: None,
            rss: None,
            available: None,
            complete: false,
            blocked: false,
        })
    }
    pub fn reconfigure(&mut self, configuration: ResourceConfiguration) -> Result<()> {
        let proposed = Self::new(configuration, self.cpus, self.memory)?;
        // Existing charges remain in place; a lower limit stops new grants until drain.
        self.configuration = proposed.configuration;
        self.limit = proposed.limit;
        self.source_threads = proposed.source_threads;
        self.inference_threads = proposed.inference_threads;
        self.resident_limit = proposed.resident_limit;
        Ok(())
    }
    fn reserved(&self) -> u64 {
        MODEL_BYTES
            + SERVICE_BYTES
            + self.residents.len() as u64 * RESIDENT_BYTES
            + self
                .running
                .values()
                .map(|grant| grant.working_memory_bytes)
                .sum::<u64>()
    }
    fn pressure(&self) -> bool {
        self.reserved() > self.limit
            || self.rss.is_some_and(|rss| rss >= self.limit)
            || self
                .available
                .is_some_and(|available| available < 128 << 20)
    }
    fn headroom(&self, additional: u64) -> bool {
        !self.blocked
            && !self.pressure()
            && self
                .reserved()
                .max(self.rss.unwrap_or(0))
                .saturating_add(additional)
                <= self.limit
    }
    pub fn block_admission(&mut self, blocked: bool) {
        self.blocked = blocked;
    }
    pub fn observe(&mut self, rss: Option<u64>, available: Option<u64>, complete: bool) {
        self.rss = rss;
        self.available = available;
        self.complete = complete;
    }
    pub fn may_start_resident(&self) -> bool {
        self.residents.len() < self.resident_limit && self.headroom(RESIDENT_BYTES)
    }
    pub fn resident_started(&mut self, identity: WorkerIdentity) -> Result<()> {
        ensure!(
            !self.residents.contains_key(&identity.project) && self.may_start_resident(),
            "service resident capacity busy"
        );
        self.residents.insert(identity.project, identity);
        Ok(())
    }
    /// Only a confirmed process exit may return a resident and any unacknowledged work.
    pub fn process_exited(&mut self, identity: WorkerIdentity) -> Result<()> {
        ensure!(
            self.residents.get(&identity.project) == Some(&identity),
            "foreign service resource exit"
        );
        self.residents.remove(&identity.project);
        self.running.remove(&identity.project);
        Ok(())
    }
    pub fn pending(
        &mut self,
        project: ProjectBinding,
        job: Sha256Digest,
        kind: JobKind,
        now: Instant,
    ) -> Result<()> {
        ensure!(
            self.waiting.len() < 1024 || self.waiting.contains_key(&project),
            "service waiting-project bound exceeded"
        );
        if self
            .waiting
            .get(&project)
            .is_some_and(|waiting| waiting.job == job)
        {
            return Ok(());
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("service scheduling sequence exhausted"))?;
        self.waiting.insert(
            project,
            Waiting {
                job,
                kind,
                first_seen: now,
                sequence: self.sequence,
            },
        );
        Ok(())
    }
    pub fn forget_pending(&mut self, project: ProjectBinding) {
        self.waiting.remove(&project);
    }
    pub fn running(&self, project: ProjectBinding) -> Option<&WorkAllowance> {
        self.running.get(&project)
    }
    pub fn next(&mut self, now: Instant) -> Result<Option<WorkAllowance>> {
        let used = self
            .running
            .values()
            .map(|grant| grant.extraction_threads)
            .sum::<u16>();
        if used >= self.source_threads || !self.headroom(WORK_BYTES) {
            return Ok(None);
        }
        let candidate = self
            .waiting
            .iter()
            .filter(|(project, _)| {
                self.residents.contains_key(project) && !self.running.contains_key(project)
            })
            .min_by_key(|(project, waiting)| {
                let aged =
                    now.saturating_duration_since(waiting.first_seen) >= Duration::from_secs(5);
                let rank = if aged {
                    0
                } else {
                    match waiting.kind {
                        JobKind::Index | JobKind::Rebuild | JobKind::Recover => 1,
                        JobKind::Reconcile => 2,
                        JobKind::Vectors => 3,
                        JobKind::Inspect | JobKind::Verify => 4,
                        JobKind::Maintain => 5,
                    }
                };
                (rank, Some(**project) == self.last_project, waiting.sequence)
            })
            .map(|(project, _)| *project);
        let Some(project) = candidate else {
            return Ok(None);
        };
        let waiting = self
            .waiting
            .remove(&project)
            .expect("selected waiting project");
        let mut threads = ((self.source_threads - used)
            / (self.waiting.len().min(u16::MAX as usize) as u16 + 1))
            .clamp(1, 4);
        while threads > 1
            && !self.headroom(WORK_BYTES + u64::from(threads - 1) * EXTRA_EXTRACTOR_BYTES)
        {
            threads -= 1;
        }
        let grant = WorkAllowance {
            id: crate::service_broker::new_instance_id()?,
            identity: self.residents[&project],
            job: waiting.job,
            extraction_threads: threads,
            inference_threads: self.inference_threads,
            working_memory_bytes: WORK_BYTES + u64::from(threads - 1) * EXTRA_EXTRACTOR_BYTES,
            background_io_bytes_per_second: self
                .configuration
                .background_io_bytes_per_second
                .unwrap_or(match self.configuration.preset {
                    ResourcePreset::Responsive => 8 << 20,
                    ResourcePreset::Balanced => 32 << 20,
                    ResourcePreset::Throughput => 128 << 20,
                }),
            ttl_ms: 300_000,
        };
        grant.validate()?;
        self.last_project = Some(project);
        self.running.insert(project, grant.clone());
        Ok(Some(grant))
    }
    /// Expiry/timeout alone never invokes this acknowledgement.
    pub fn finished(&mut self, identity: WorkerIdentity, id: Incarnation) -> Result<()> {
        ensure!(
            self.running
                .get(&identity.project)
                .is_some_and(|grant| grant.identity == identity && grant.id == id),
            "foreign service work completion"
        );
        self.running.remove(&identity.project);
        Ok(())
    }
    pub fn snapshot(&self) -> ResourceSnapshot {
        ResourceSnapshot {
            configuration: self.configuration.clone(),
            logical_cpus: self.cpus,
            detected_memory_bytes: self.memory,
            resident_limit: self.resident_limit,
            resident_workers: self.residents.len(),
            extraction_threads_limit: self.source_threads,
            inference_threads_limit: self.inference_threads,
            extraction_threads_reserved: self
                .running
                .values()
                .map(|grant| grant.extraction_threads)
                .sum(),
            memory_limit_bytes: self.limit,
            memory_reserved_bytes: self.reserved(),
            observed_rss_bytes: self.rss,
            available_memory_bytes: self.available,
            telemetry_complete: self.complete,
            pressure: self.pressure(),
            running_jobs: self.running.len(),
            waiting_projects: self.waiting.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn identity(n: u8) -> WorkerIdentity {
        WorkerIdentity {
            service: Incarnation::from_bytes([1; 16]),
            worker: Incarnation::from_bytes([n; 16]),
            project: Sha256Digest::from_bytes([n; 32]),
            fencing_generation: 1,
        }
    }
    fn resources() -> Result<Resources> {
        Resources::new(
            ResourceConfiguration {
                extraction_threads: Some(1),
                memory_bytes: Some(8 << 30),
                ..Default::default()
            },
            8,
            Some(16 << 30),
        )
    }
    #[test]
    fn detected_machine_limit_wins_over_defaults_and_overrides() -> Result<()> {
        let mut policy = Resources::new(
            ResourceConfiguration {
                memory_bytes: Some(8 << 30),
                ..Default::default()
            },
            8,
            Some(512 << 20),
        )?;
        assert_eq!(policy.snapshot().memory_limit_bytes, 512 << 20);
        assert!(!policy.may_start_resident());
        let a = identity(2);
        policy.pending(a.project, a.project, JobKind::Index, Instant::now())?;
        assert!(policy.next(Instant::now())?.is_none());
        Ok(())
    }
    #[test]
    fn expired_grants_and_foreign_acknowledgements_cannot_return_capacity() -> Result<()> {
        let mut policy = resources()?;
        let a = identity(2);
        let b = identity(3);
        let now = Instant::now();
        for identity in [a, b] {
            policy.resident_started(identity)?;
            policy.pending(identity.project, identity.project, JobKind::Index, now)?;
        }
        let grant = policy.next(now)?.unwrap();
        assert_eq!(grant.identity, a);
        assert!(policy.next(now + Duration::from_secs(600))?.is_none());
        assert!(policy.finished(b, grant.id).is_err());
        assert!(
            policy
                .finished(a, Incarnation::from_bytes([9; 16]))
                .is_err()
        );
        assert_eq!(policy.snapshot().running_jobs, 1);
        policy.process_exited(a)?;
        assert_eq!(policy.next(now)?.unwrap().identity, b);
        Ok(())
    }
    #[test]
    fn old_verification_cannot_starve_under_continuous_source_work() -> Result<()> {
        let mut policy = resources()?;
        let a = identity(2);
        let b = identity(3);
        let now = Instant::now();
        for identity in [a, b] {
            policy.resident_started(identity)?;
        }
        policy.pending(b.project, b.project, JobKind::Verify, now)?;
        policy.pending(a.project, a.project, JobKind::Index, now)?;
        let grant = policy.next(now)?.unwrap();
        assert_eq!(grant.identity, a);
        policy.finished(a, grant.id)?;
        policy.pending(
            a.project,
            Sha256Digest::from_bytes([9; 32]),
            JobKind::Index,
            now + Duration::from_secs(6),
        )?;
        assert_eq!(
            policy.next(now + Duration::from_secs(6))?.unwrap().identity,
            b
        );
        Ok(())
    }
    #[test]
    fn observed_pressure_and_lowered_limits_stop_new_grants_without_releasing_old() -> Result<()> {
        let mut policy = resources()?;
        let a = identity(2);
        let now = Instant::now();
        policy.resident_started(a)?;
        policy.pending(a.project, a.project, JobKind::Rebuild, now)?;
        policy.observe(Some(8 << 30), Some(2 << 30), true);
        assert!(policy.next(now)?.is_none());
        policy.observe(Some(100 << 20), Some(2 << 30), true);
        let grant = policy.next(now)?.unwrap();
        let before = policy.snapshot().memory_reserved_bytes;
        policy.reconfigure(ResourceConfiguration {
            memory_bytes: Some(1 << 30),
            ..Default::default()
        })?;
        assert_eq!(policy.snapshot().memory_reserved_bytes, before);
        assert_eq!(policy.running(a.project).unwrap().id, grant.id);
        policy.observe(None, None, false);
        assert!(!policy.snapshot().telemetry_complete);
        assert_eq!(policy.snapshot().observed_rss_bytes, None);
        Ok(())
    }
    #[test]
    fn one_project_can_use_bounded_parallel_extraction_and_residents_exceed_four() -> Result<()> {
        let mut policy = Resources::new(
            ResourceConfiguration {
                preset: ResourcePreset::Throughput,
                ..Default::default()
            },
            32,
            Some(32 << 30),
        )?;
        for n in 2..10 {
            policy.resident_started(identity(n))?;
        }
        assert_eq!(policy.snapshot().resident_workers, 8);
        let now = Instant::now();
        let a = identity(2);
        policy.pending(a.project, a.project, JobKind::Index, now)?;
        assert_eq!(policy.next(now)?.unwrap().extraction_threads, 4);
        Ok(())
    }
}
