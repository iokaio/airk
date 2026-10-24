// SPDX-License-Identifier: Apache-2.0
//! Durable content-free provider ownership; no command, source or query payloads.
use super::ServiceRegistry;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::WorkerIdentity,
};
use anyhow::{Result, ensure};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS service_provider_drains(
 id TEXT PRIMARY KEY CHECK(length(id)=32),
 payload BLOB NOT NULL CHECK(length(payload)<=4096),
 digest TEXT NOT NULL CHECK(length(digest)=64)) WITHOUT ROWID;";
pub const MAX_PROVIDER_DRAINS: usize = 128;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProviderBoot {
    pub boot_id: Option<String>,
    pub process_scope: String,
    pub elapsed_ns: u64,
    pub monotonic_reset_proves_boot: bool,
}

impl ProviderBoot {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.boot_id.as_ref().is_none_or(|id| !id.is_empty()
                && id.len() <= 80
                && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')),
            "invalid native provider boot identity"
        );
        ensure!(
            !self.process_scope.is_empty()
                && self.process_scope.len() <= 128
                && !self.process_scope.chars().any(char::is_control),
            "invalid native provider process scope"
        );
        Ok(())
    }
    fn changed_from(&self, old: &Self) -> bool {
        match (&self.boot_id, &old.boot_id) {
            (Some(current), Some(previous)) => current != previous,
            _ => {
                self.monotonic_reset_proves_boot
                    && old.monotonic_reset_proves_boot
                    && self.process_scope == old.process_scope
                    && self.elapsed_ns < old.elapsed_ns
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProviderDrain {
    pub version: u32,
    pub id: Incarnation,
    pub namespace: Incarnation,
    pub identity: WorkerIdentity,
    pub guardian_pid: u32,
    pub group: Option<u32>,
    pub boot: ProviderBoot,
}
impl ProviderDrain {
    fn bytes(&self) -> Result<Vec<u8>> {
        ensure!(
            self.version == 1
                && self.guardian_pid > 1
                && self.guardian_pid <= i32::MAX as u32
                && self
                    .group
                    .is_none_or(|pid| pid > 1 && pid <= i32::MAX as u32)
                && self.identity.fencing_generation > 0,
            "invalid provider drain identity"
        );
        self.boot.validate()?;
        let bytes = serde_json::to_vec(self)?;
        ensure!(bytes.len() <= 4096, "provider drain metadata exceeds bound");
        Ok(bytes)
    }
}
fn digest(bytes: &[u8]) -> String {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into()).to_string()
}
fn decode(
    id: String,
    bytes: Vec<u8>,
    hash: String,
    namespace: Incarnation,
) -> Result<ProviderDrain> {
    ensure!(
        bytes.len() <= 4096 && hash == digest(&bytes),
        "provider drain metadata digest mismatch"
    );
    let record: ProviderDrain = serde_json::from_slice(&bytes)?;
    ensure!(
        record.id.to_string() == id && record.namespace == namespace && record.bytes()? == bytes,
        "provider drain ownership mismatch"
    );
    Ok(record)
}
impl ServiceRegistry {
    pub fn begin_provider_drain(
        &mut self,
        identity: WorkerIdentity,
        id: Incarnation,
        guardian_pid: u32,
        boot: ProviderBoot,
    ) -> Result<ProviderDrain> {
        ensure!(
            self.get(identity.project)?.fencing_generation == identity.fencing_generation,
            "stale provider worker generation"
        );
        let record = ProviderDrain {
            version: 1,
            id,
            namespace: self.incarnation()?,
            identity,
            guardian_pid,
            group: None,
            boot,
        };
        let bytes = record.bytes()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count: i64 =
            transaction.query_row("SELECT count(*) FROM service_provider_drains", [], |row| {
                row.get(0)
            })?;
        ensure!(
            (0..MAX_PROVIDER_DRAINS as i64).contains(&count),
            "provider drain admission limit reached"
        );
        transaction.execute(
            "INSERT INTO service_provider_drains VALUES(?1,?2,?3)",
            params![id.to_string(), bytes, digest(&bytes)],
        )?;
        transaction.commit()?;
        Ok(record)
    }
    pub fn bind_provider_group(
        &mut self,
        record: &ProviderDrain,
        group: u32,
    ) -> Result<ProviderDrain> {
        ensure!(
            record.namespace == self.incarnation()? && record.group.is_none() && group > 1,
            "invalid provider group transition"
        );
        let old = record.bytes()?;
        let mut next = record.clone();
        next.group = Some(group);
        let bytes = next.bytes()?;
        let changed=self.connection.execute("UPDATE service_provider_drains SET payload=?1,digest=?2 WHERE id=?3 AND payload=?4 AND digest=?5",
            params![bytes,digest(&bytes),record.id.to_string(),old,digest(&old)])?;
        ensure!(changed == 1, "provider group ownership changed");
        Ok(next)
    }
    /// Only the native parent calls this after actual owned group drain, or a
    /// failed spawn that never returned a child. Missing completion is idempotent.
    pub fn complete_provider_drain(&mut self, record: &ProviderDrain) -> Result<()> {
        ensure!(
            record.namespace == self.incarnation()?,
            "foreign provider completion namespace"
        );
        let bytes = record.bytes()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: Option<Vec<u8>> = transaction
            .query_row(
                "SELECT substr(payload,1,4097) FROM service_provider_drains WHERE id=?1",
                [record.id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(current) = current {
            ensure!(current == bytes, "foreign provider completion identity");
            let removed = transaction.execute(
                "DELETE FROM service_provider_drains WHERE id=?1 AND digest=?2",
                params![record.id.to_string(), digest(&bytes)],
            )?;
            ensure!(removed == 1, "provider completion digest mismatch");
        }
        transaction.commit()?;
        Ok(())
    }
    pub fn provider_drains(&self) -> Result<Vec<ProviderDrain>> {
        let namespace = self.incarnation()?;
        let mut statement=self.connection.prepare("SELECT substr(id,1,33),substr(payload,1,4097),substr(digest,1,65) FROM service_provider_drains ORDER BY id LIMIT 129")?;
        let mut records = Vec::new();
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            ensure!(
                records.len() < MAX_PROVIDER_DRAINS,
                "provider drain metadata count exceeded"
            );
            records.push(decode(row.get(0)?, row.get(1)?, row.get(2)?, namespace)?);
        }
        Ok(records)
    }
    /// Absence observations never signal an unowned/reused group. A failed,
    /// foreign-scope or unbound observation preserves the durable charge.
    pub fn recover_provider_drains(
        &mut self,
        current: &ProviderBoot,
        active: &[WorkerIdentity],
        mut absent: impl FnMut(u32) -> Result<bool>,
    ) -> Result<usize> {
        current.validate()?;
        ensure!(active.len() <= 32, "active provider worker bound exceeded");
        let records = self.provider_drains()?;
        let mut pending = 0;
        for record in records {
            if active.contains(&record.identity) {
                continue;
            }
            let gone = current.changed_from(&record.boot)
                || match record.group {
                    Some(group) if current.process_scope == record.boot.process_scope => {
                        absent(group)?
                    }
                    _ => false,
                };
            if gone {
                self.complete_provider_drain(&record)?;
            } else {
                pending += 1;
            }
        }
        Ok(pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::ProjectLease;
    use airk_core::service::ProjectRegistration;
    fn fixture() -> Result<(
        tempfile::TempDir,
        ServiceRegistry,
        ProjectLease,
        WorkerIdentity,
    )> {
        let root = tempfile::tempdir()?;
        let service = Incarnation::from_bytes([1; 16]);
        let mut registry = ServiceRegistry::open(root.path(), service)?;
        let registration = ProjectRegistration::new(
            "/provider-fixture".into(),
            "/provider-derived".into(),
            "fixture".into(),
        )?;
        registry.register(&registration)?;
        let mut lease = ProjectLease::acquire(root.path(), registration.binding)?;
        let generation = registry.admit_worker(&mut lease)?;
        let identity = WorkerIdentity {
            service,
            worker: Incarnation::from_bytes([2; 16]),
            project: registration.binding,
            fencing_generation: generation,
        };
        Ok((root, registry, lease, identity))
    }
    fn boot() -> ProviderBoot {
        ProviderBoot {
            boot_id: Some("aaaa".into()),
            process_scope: "pid:[42]".into(),
            elapsed_ns: 100,
            monotonic_reset_proves_boot: false,
        }
    }
    fn begin(registry: &mut ServiceRegistry, identity: WorkerIdentity) -> Result<ProviderDrain> {
        registry.begin_provider_drain(identity, Incarnation::from_bytes([3; 16]), 123, boot())
    }
    #[test]
    fn provider_records_are_fenced_durable_and_completed_exactly() -> Result<()> {
        let (root, mut registry, _lease, identity) = fixture()?;
        let mut stale = identity;
        stale.fencing_generation += 1;
        assert!(begin(&mut registry, stale).is_err());
        assert!(
            registry
                .begin_provider_drain(identity, Incarnation::from_bytes([8; 16]), u32::MAX, boot())
                .is_err()
        );
        let pending = begin(&mut registry, identity)?;
        assert!(registry.bind_provider_group(&pending, 1).is_err());
        assert!(registry.bind_provider_group(&pending, u32::MAX).is_err());
        let record = registry.bind_provider_group(&pending, 456)?;
        assert!(registry.bind_provider_group(&pending, 789).is_err());
        assert!(registry.complete_provider_drain(&pending).is_err());
        drop(registry);
        let mut registry = ServiceRegistry::open(root.path(), identity.service)?;
        assert_eq!(registry.provider_drains()?, vec![record.clone()]);
        registry.complete_provider_drain(&record)?;
        registry.complete_provider_drain(&record)?;
        assert!(registry.provider_drains()?.is_empty());
        Ok(())
    }
    #[test]
    #[cfg(unix)]
    fn guardian_pin_survives_original_owner_drop_until_last_descriptor_close() -> Result<()> {
        let (root, _registry, lease, identity) = fixture()?;
        let pin = lease.duplicate_native_pin()?;
        drop(lease);
        assert!(ProjectLease::acquire(root.path(), identity.project).is_err());
        let second = pin.try_clone()?;
        drop(pin);
        assert!(ProjectLease::acquire(root.path(), identity.project).is_err());
        drop(second);
        ProjectLease::acquire(root.path(), identity.project)?;
        Ok(())
    }
    #[test]
    fn unbound_and_active_records_never_expire_by_time() -> Result<()> {
        let (_root, mut registry, _lease, identity) = fixture()?;
        begin(&mut registry, identity)?;
        let mut later = boot();
        later.elapsed_ns = u64::MAX;
        assert_eq!(
            registry
                .recover_provider_drains(&later, &[], |_| panic!("unbound has no native group"))?,
            1
        );
        assert_eq!(
            registry.recover_provider_drains(&later, &[identity], |_| panic!(
                "active must retain parent ownership"
            ))?,
            0
        );
        assert_eq!(registry.provider_drains()?.len(), 1);
        Ok(())
    }
    #[test]
    fn group_absence_requires_same_process_scope_and_no_active_owner() -> Result<()> {
        let (_root, mut registry, _lease, identity) = fixture()?;
        let record = begin(&mut registry, identity)?;
        registry.bind_provider_group(&record, 456)?;
        let mut foreign = boot();
        foreign.process_scope = "pid:[99]".into();
        assert_eq!(
            registry.recover_provider_drains(&foreign, &[], |_| panic!("foreign PID scope"))?,
            1
        );
        assert_eq!(
            registry.recover_provider_drains(&boot(), &[], |pid| {
                assert_eq!(pid, 456);
                Ok(false)
            })?,
            1
        );
        assert!(
            registry
                .recover_provider_drains(&boot(), &[], |_| anyhow::bail!("native denial"))
                .is_err()
        );
        assert_eq!(registry.provider_drains()?.len(), 1);
        assert_eq!(
            registry.recover_provider_drains(&boot(), &[], |_| Ok(true))?,
            0
        );
        Ok(())
    }
    #[test]
    fn only_positive_boot_evidence_clears_an_unbound_record() -> Result<()> {
        let (_root, mut registry, _lease, identity) = fixture()?;
        begin(&mut registry, identity)?;
        let mut current = boot();
        current.elapsed_ns = 0;
        assert_eq!(
            registry.recover_provider_drains(&current, &[], |_| panic!())?,
            1
        );
        current.boot_id = Some("bbbb".into());
        assert_eq!(
            registry.recover_provider_drains(&current, &[], |_| panic!())?,
            0
        );
        let mut old = boot();
        old.boot_id = None;
        old.monotonic_reset_proves_boot = true;
        registry.begin_provider_drain(
            identity,
            Incarnation::from_bytes([4; 16]),
            123,
            old.clone(),
        )?;
        let mut current = old.clone();
        current.elapsed_ns += 1;
        assert_eq!(
            registry.recover_provider_drains(&current, &[], |_| panic!())?,
            1
        );
        current.elapsed_ns = old.elapsed_ns - 1;
        assert_eq!(
            registry.recover_provider_drains(&current, &[], |_| panic!())?,
            0
        );
        Ok(())
    }
    #[test]
    fn digest_damage_cannot_complete_or_recover_ownership() -> Result<()> {
        let (_root, mut registry, _lease, identity) = fixture()?;
        let record = begin(&mut registry, identity)?;
        registry.connection.execute(
            "UPDATE service_provider_drains SET digest=?1",
            ["0".repeat(64)],
        )?;
        assert!(registry.provider_drains().is_err());
        assert!(registry.complete_provider_drain(&record).is_err());
        let count: i64 = registry.connection.query_row(
            "SELECT count(*) FROM service_provider_drains",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(count, 1);
        Ok(())
    }
    #[test]
    fn pending_provider_metadata_has_a_global_capacity_bound() -> Result<()> {
        let (_root, mut registry, _lease, identity) = fixture()?;
        for n in 0..MAX_PROVIDER_DRAINS {
            registry.begin_provider_drain(
                identity,
                Incarnation::from_bytes([n as u8; 16]),
                123,
                boot(),
            )?;
        }
        assert_eq!(registry.provider_drains()?.len(), MAX_PROVIDER_DRAINS);
        assert!(
            registry
                .begin_provider_drain(identity, Incarnation::from_bytes([255; 16]), 123, boot())
                .is_err()
        );
        Ok(())
    }
}
