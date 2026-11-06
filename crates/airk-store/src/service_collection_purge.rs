// SPDX-License-Identifier: Apache-2.0
//! Bounded full deletion of an inactive, ownership-matched service collection.
use super::*;
use crate::service::ProjectLease;
use airk_core::service::ProjectRegistration;

impl Collection {
    pub(crate) fn purge_removed_turn(
        registration: &ProjectRegistration,
        lease: &ProjectLease,
    ) -> Result<bool> {
        Self::purge_removed_turn_inner(registration, lease, None, None)
    }
    pub(crate) fn purge_removed_turn_inner(
        registration: &ProjectRegistration,
        lease: &ProjectLease,
        split_access: Option<&SplitBuildPermit>,
        funding: Option<&crate::quota::funding::FundingUse>,
    ) -> Result<bool> {
        ensure!(
            lease.identity().0 == registration.binding,
            "foreign removal lease"
        );
        let directory = Path::new(&registration.data_root)
            .join("collections-1.4")
            .join(&registration.collection);
        crate::managed_collection::validate_ancestors(&directory)?;
        match fs::symlink_metadata(&directory) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
            Err(error) => return Err(error.into()),
            Ok(_) => (),
        }
        ensure!(
            plain(&directory)?.is_dir(),
            "invalid removed collection directory"
        );
        let policy = crate::current::policy_capsule::PolicyCapsule::load(&directory)?;
        if let Some(policy) = &policy {
            policy.validate_registration(registration)?;
        }
        // Owner is deleted last. A crash after that point may leave only an empty directory.
        if !directory.join("service-owner.json").try_exists()? {
            if policy.is_some() {
                for entry in fs::read_dir(&directory)? {
                    ensure!(
                        entry?.file_name() == "source-policy.json",
                        "unowned removal state is preserved"
                    );
                }
                return Ok(true);
            }
            ensure!(
                fs::read_dir(&directory)?.next().is_none(),
                "unowned removal state is preserved"
            );
            fs::remove_dir(&directory)?;
            return Ok(true);
        }
        let collection = Self::existing(
            Path::new(&registration.data_root),
            &registration.root,
            &registration.collection,
        )?;
        let retained_split = collection.owner.required_split.is_some();
        if let Some(pointer) = &collection.owner.required_split {
            super::split::validate_capsule_for_retirement(&directory, pointer, split_access)?;
        }
        // A dead funded capture's exact stage remains owned until the native
        // payload-disposal checkpoint below. Recognize ONLY the admitted ledger
        // paths; do not classify a prefix as safe or delete staging early.
        let admitted_staging = if let Some(funding) = funding {
            let permit =
                split_access.context("funded staging disposal requires claimed cleanup access")?;
            let store = FileStore::new(&directory)
                .with_split_access(permit.clone())?
                .with_operation_funding(funding.clone())?;
            Quota::for_store(&store)?.operation_staging_for_disposal(funding)?
        } else {
            std::collections::BTreeSet::new()
        };
        let _usage = crate::usage::deletion_guard(&directory)?;
        collection.sweep_metadata_scratch()?;
        let incarnations = directory.join("incarnations");
        // Prove the complete top-level scope before deleting anything.
        let mut top = Vec::new();
        for (count, entry) in fs::read_dir(&directory)?.enumerate() {
            ensure!(count < 2112, "removal metadata budget exceeded");
            let path = entry?.path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .context("invalid removal entry")?;
            if admitted_staging.contains(&path) {
                ensure!(
                    plain(&path)?.is_dir(),
                    "invalid exact funded staging directory"
                );
                continue;
            }
            if name.starts_with("split-state-") && retained_split {
                super::split::validate_retained_capsule_file(&path)?;
                continue;
            }
            if name == "incarnations" || name == ".current-gates" {
                ensure!(plain(&path)?.is_dir(), "invalid incarnation root");
                continue;
            }
            if name == "source-policy.json" && policy.is_some() {
                ensure!(plain(&path)?.is_file(), "invalid retained source policy");
                continue;
            }
            ensure!(
                matches!(
                    name,
                    "service-owner.json"
                        | "integrity-observation.json"
                        | "integrity-recovery.json"
                        | "integrity-projection.json"
                        | "active.json"
                        | "replacement.json"
                        | "initial-slot.json"
                        | "derived-cleanup.json"
                        | "manifest.lock"
                        | "replacement.lock"
                        | "usage.lock"
                        | "usage-policy.json"
                        | "usage.sqlite3"
                        | "usage.sqlite3-wal"
                        | "usage.sqlite3-shm"
                        | "usage.sqlite3-journal"
                ),
                "unknown removal entry is preserved: {name}"
            );
            ensure!(plain(&path)?.is_file(), "invalid removal metadata");
            top.push(path);
        }
        if incarnations.try_exists()? {
            let mut slots = Vec::new();
            for (count, entry) in fs::read_dir(&incarnations)?.enumerate() {
                ensure!(count < 16, "removal incarnation budget exceeded");
                let path = entry?.path();
                let slot: Incarnation = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .context("invalid removal slot")?
                    .parse()?;
                ensure!(plain(&path)?.is_dir(), "invalid removal slot directory");
                slots.push((slot, path));
            }
            // The owner binds the entire collection. Each slot additionally uses
            // the existing allowlisted inventory, writer lock and request gate.
            let last_slot = slots.len() == 1;
            if let Some((_, path)) = slots.into_iter().next() {
                let mut store = FileStore::new(&path);
                if let Some(permit) = split_access {
                    store = store.with_split_access(permit.clone())?;
                }
                if let Some(funding) = funding {
                    store = store.with_operation_funding(funding.clone())?;
                }
                let writer = store.lock()?;
                let incarnation = if path.join("current-owner.json").try_exists()? {
                    Some(
                        store
                            .current_scope(&collection.owner.root, &collection.owner.collection)?
                            .context("removal slot ownership missing")?
                            .incarnation,
                    )
                } else {
                    None
                };
                let gate = Gate::exclusive(&store, &writer)?;
                let incarnation = if funding.is_some() {
                    store.service_incarnation()?
                } else {
                    incarnation
                };
                let quota = Quota::for_store(&store)?;
                if let Some(funding) = funding {
                    ensure!(
                        quota
                            .funded_children(funding)?
                            .iter()
                            .all(|child| child.closed
                                || !matches!(
                                    child.kind,
                                    crate::quota::funding::FundingChildKind::Reservation { .. }
                                )),
                        "split cleanup must explicitly adopt/reconcile captured reservations before disposal"
                    );
                }
                // Refuse a crossed/ordinary quota before deleting a journal or
                // any payload needed to recover an active funded child.
                for grant in quota.current_grants(&writer)? {
                    if (funding.is_none() && last_slot)
                        || Some(grant.scope().incarnation) == incarnation
                    {
                        grant.require_release_authority()?;
                    }
                }
                let (files, mut directories) = retirement_inventory(&path)?;
                let ordinary: Vec<_> = files
                    .iter()
                    .filter(|file| {
                        file.file_name()
                            .is_none_or(|name| name != "current-owner.json")
                    })
                    .collect();
                for file in ordinary.iter().take(4096) {
                    fs::remove_file(file)?;
                }
                // The inventory's extra entry is a continuation sentinel. Keep
                // it even when the preserved owner reduces the deletion count.
                if files.len() > 4096 {
                    return Ok(false);
                }
                if funding.is_some() {
                    store.reconcile_disposed_operation_funding(&writer, &quota, &gate)?;
                }
                for grant in quota.current_grants(&writer)? {
                    if (funding.is_none() && last_slot)
                        || Some(grant.scope().incarnation) == incarnation
                    {
                        grant.release(&writer)?;
                    }
                }
                if path.join("current-owner.json").try_exists()? {
                    fs::remove_file(path.join("current-owner.json"))?;
                }
                directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
                for path in directories {
                    fs::remove_dir(&path)
                        .with_context(|| format!("remove owned directory {}", path.display()))?;
                }
                // Bound each turn to one incarnation; the registry guard persists.
                return Ok(false);
            }
            fs::remove_dir(&incarnations)?;
        }
        // Last-incarnation disposal normally removed these through canonical
        // journal/quota reconciliation on the prior turn. Never report complete
        // from a missing incarnation if its actual funded staging still exists.
        for stage in &admitted_staging {
            match fs::symlink_metadata(stage) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
                Ok(_) => anyhow::bail!("admitted funded staging awaits native disposal settlement"),
            }
        }
        let gates = directory.join(".current-gates");
        if gates.try_exists()? {
            let mut gate_files = Vec::new();
            for (count, entry) in fs::read_dir(&gates)?.enumerate() {
                ensure!(count < 4096, "removal gate inventory budget exceeded");
                let path = entry?.path();
                let _: Incarnation = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .context("invalid removal gate")?
                    .parse()?;
                ensure!(plain(&path)?.is_file(), "invalid removal gate file");
                gate_files.push(path);
            }
            for path in gate_files {
                let file = fs::OpenOptions::new().read(true).write(true).open(&path)?;
                file.try_lock()?;
                fs::remove_file(path)?;
            }
            fs::remove_dir(gates)?;
        }
        top.sort_by_key(|path| {
            path.file_name()
                .is_some_and(|name| name == "service-owner.json")
        });
        for path in top {
            if (policy.is_some() || retained_split)
                && path
                    .file_name()
                    .is_some_and(|name| name == "service-owner.json")
            {
                continue;
            }
            fs::remove_file(path)?;
        }
        if policy.is_none() && !retained_split {
            fs::remove_dir(&directory)?;
        }
        Ok(true)
    }
}
