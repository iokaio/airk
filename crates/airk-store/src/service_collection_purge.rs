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
        // Owner is deleted last. A crash after that point may leave only an empty directory.
        if !directory.join("service-owner.json").try_exists()? {
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
        let _usage = crate::usage::deletion_guard(&directory)?;
        collection.sweep_metadata_scratch()?;
        let incarnations = directory.join("incarnations");
        // Prove the complete top-level scope before deleting anything.
        let mut top = Vec::new();
        for (count, entry) in fs::read_dir(&directory)?.enumerate() {
            ensure!(count < 64, "removal metadata budget exceeded");
            let path = entry?.path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .context("invalid removal entry")?;
            if name == "incarnations" || name == ".current-gates" {
                ensure!(plain(&path)?.is_dir(), "invalid incarnation root");
                continue;
            }
            ensure!(
                matches!(
                    name,
                    "service-owner.json"
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
                let store = FileStore::new(&path);
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
                let _gate = Gate::exclusive(&store, &writer)?;
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
                let quota = Quota::open_service(Path::new(&registration.data_root))?;
                for grant in quota.current_grants(&writer)? {
                    if last_slot || Some(grant.scope().incarnation) == incarnation {
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
            fs::remove_file(path)?;
        }
        fs::remove_dir(&directory)?;
        Ok(true)
    }
}
