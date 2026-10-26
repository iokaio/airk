// SPDX-License-Identifier: Apache-2.0
//! Typed provider ownership; absence observations never signal foreign groups.
use super::super::{MAX_PROVIDER_DRAINS, ProviderBoot, ProviderDrain};
use super::{NativeServiceRegistry, State};
use airk_core::{identity::Incarnation, service::WorkerIdentity};
use anyhow::{Result, ensure};

pub(super) fn validate(state: &State) -> Result<()> {
    ensure!(
        state.providers.len() <= MAX_PROVIDER_DRAINS,
        "provider drain metadata count exceeded"
    );
    for (id, record) in &state.providers {
        record.bytes()?;
        ensure!(
            id == &record.id.to_string() && record.namespace == state.request_namespace,
            "provider drain ownership mismatch"
        );
    }
    Ok(())
}
impl NativeServiceRegistry {
    pub fn begin_provider_drain(
        &mut self,
        identity: WorkerIdentity,
        id: Incarnation,
        guardian_pid: u32,
        boot: ProviderBoot,
    ) -> Result<ProviderDrain> {
        self.write(|state| {
            ensure!(
                state
                    .projects
                    .get(&identity.project.to_string())
                    .is_some_and(|project| project.registered
                        && project.epoch == identity.fencing_generation),
                "stale provider worker generation"
            );
            ensure!(
                state.providers.len() < MAX_PROVIDER_DRAINS,
                "provider drain admission limit reached"
            );
            ensure!(
                !state.providers.contains_key(&id.to_string()),
                "provider drain identity already exists"
            );
            let record = ProviderDrain {
                version: 1,
                id,
                namespace: state.request_namespace,
                identity,
                guardian_pid,
                group: None,
                boot,
            };
            record.bytes()?;
            state.providers.insert(id.to_string(), record.clone());
            state.bump()?;
            Ok(record)
        })
    }
    pub fn bind_provider_group(
        &mut self,
        record: &ProviderDrain,
        group: u32,
    ) -> Result<ProviderDrain> {
        self.write(|state| {
            ensure!(
                record.namespace == state.request_namespace && record.group.is_none() && group > 1,
                "invalid provider group transition"
            );
            record.bytes()?;
            ensure!(
                state.providers.get(&record.id.to_string()) == Some(record),
                "provider group ownership changed"
            );
            let mut next = record.clone();
            next.group = Some(group);
            next.bytes()?;
            state.providers.insert(record.id.to_string(), next.clone());
            state.bump()?;
            Ok(next)
        })
    }
    pub fn complete_provider_drain(&mut self, record: &ProviderDrain) -> Result<()> {
        self.write(|state| {
            ensure!(
                record.namespace == state.request_namespace,
                "foreign provider completion namespace"
            );
            record.bytes()?;
            if let Some(current) = state.providers.get(&record.id.to_string()) {
                ensure!(current == record, "foreign provider completion identity");
                state.providers.remove(&record.id.to_string());
                state.bump()?;
            }
            Ok(())
        })
    }
    pub fn provider_drains(&self) -> Result<Vec<ProviderDrain>> {
        self.read(|state| Ok(state.providers.values().cloned().collect()))
    }
    pub fn recover_provider_drains(
        &mut self,
        current: &ProviderBoot,
        active: &[WorkerIdentity],
        mut absent: impl FnMut(u32) -> Result<bool>,
    ) -> Result<usize> {
        current.validate()?;
        ensure!(active.len() <= 32, "active provider worker bound exceeded");
        let mut pending = 0;
        for record in self.provider_drains()? {
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
    #[test]
    fn provider_receipts_reject_stale_completions_and_preserve_unproven_groups() -> Result<()> {
        let root = tempfile::tempdir()?;
        let source = tempfile::tempdir()?;
        let mut registry =
            NativeServiceRegistry::open(root.path(), Incarnation::from_bytes([71; 16]))?;
        let project = ProjectRegistration::new(
            source.path().to_string_lossy().into_owned(),
            root.path()
                .with_extension("airk-data")
                .to_string_lossy()
                .into_owned(),
            "provider".into(),
        )?;
        registry.register(&project)?;
        let mut lease = ProjectLease::acquire(root.path(), project.binding)?;
        let epoch = registry.admit_worker(&mut lease)?;
        let identity = WorkerIdentity {
            project: project.binding,
            fencing_generation: epoch,
            service: Incarnation::from_bytes([74; 16]),
            worker: Incarnation::from_bytes([75; 16]),
        };
        let boot = ProviderBoot {
            boot_id: Some("0011-abcd".into()),
            process_scope: "fixture-scope".into(),
            elapsed_ns: 1000,
            monotonic_reset_proves_boot: false,
        };
        let record = registry.begin_provider_drain(
            identity,
            Incarnation::from_bytes([72; 16]),
            123,
            boot.clone(),
        )?;
        let bound = registry.bind_provider_group(&record, 456)?;
        assert!(registry.complete_provider_drain(&record).is_err());
        assert_eq!(
            registry.recover_provider_drains(&boot, &[], |_| Ok(false))?,
            1
        );
        drop(registry);
        let mut registry =
            NativeServiceRegistry::open(root.path(), Incarnation::from_bytes([73; 16]))?;
        assert_eq!(registry.provider_drains()?, vec![bound.clone()]);
        assert_eq!(
            registry.recover_provider_drains(&boot, &[identity], |_| anyhow::bail!(
                "active owner must not be probed"
            ))?,
            0
        );
        assert_eq!(
            registry.recover_provider_drains(&boot, &[], |_| Ok(true))?,
            0
        );
        registry.complete_provider_drain(&bound)?;
        assert!(registry.provider_drains()?.is_empty());
        Ok(())
    }
}
