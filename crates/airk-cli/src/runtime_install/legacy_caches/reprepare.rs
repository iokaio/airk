// SPDX-License-Identifier: Apache-2.0
//! Explicit CAS replacement of an accepted cache roster, retaining prior proof.
use super::*;

const MAX_REPLAN_BYTES: u64 = 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Replan {
    schema: u32,
    previous: Journal,
    next: Journal,
}

impl Replan {
    fn validate_transition(&self) -> Result<()> {
        let old = &self.previous.inspection;
        let next = &self.next.inspection;
        ensure!(
            self.schema == 1
                && old.schema == next.schema
                && old.active == next.active
                && old.user == next.user
                && old.scope == next.scope
                && old.context_global_storage == next.context_global_storage
                && self.previous.withdrawal_operation.is_none()
                && self.next.withdrawal_operation.is_none()
                && self.previous.image_exclusions == self.next.image_exclusions
                && next.origin_proofs.len() <= 128
                && old.caches.iter().all(|entry| next.caches.contains(entry))
                && old
                    .origin_proofs
                    .iter()
                    .all(|entry| next.origin_proofs.contains(entry))
                && old
                    .origin_locators
                    .iter()
                    .all(|entry| next.origin_locators.contains(entry))
                && old
                    .global_storage_roots
                    .iter()
                    .all(|entry| next.global_storage_roots.contains(entry))
                && self.previous.inspection_sha256 != self.next.inspection_sha256,
            "cache replan dropped or changed prior native proof"
        );
        Ok(())
    }
}

fn name(active: &str, previous: &str) -> String {
    format!("legacy-cache-replan-{active}-{previous}.json")
}

fn next_inspection(previous: &Inspection, input: Option<&Path>) -> Result<Inspection> {
    let until = std::time::Instant::now() + Duration::from_secs(60);
    let mut next = previous.clone();
    let mut bytes = 0u64;
    if previous.scope == "native-defaults" {
        next.global_storage_roots
            .extend(origin::native_cache_roots()?);
    }
    for locator in locators(input)? {
        ensure!(
            std::time::Instant::now() < until,
            "cache replan deadline exhausted"
        );
        let mut proof = origin::inspect_declared_cache(&locator)?;
        let config_bytes = fs::symlink_metadata(&locator.path)?.len();
        bytes = bytes
            .checked_add(fs::symlink_metadata(proof.executable())?.len())
            .and_then(|bytes| bytes.checked_add(config_bytes))
            .context("cache replan origin verification size overflow")?;
        ensure!(
            bytes <= 2 * 1024 * 1024 * 1024 && std::time::Instant::now() < until,
            "cache replan origin inspection bound exhausted"
        );
        proof.revalidate()?;
        let observed = proof.observation();
        next.global_storage_roots
            .push(origin::verified_global_storage(&observed.global_storage)?);
        if !next.origin_proofs.contains(observed) {
            next.origin_proofs.push(observed.clone());
        }
        if !next.origin_locators.contains(&locator) {
            next.origin_locators.push(locator);
        }
        ensure!(
            next.origin_locators.len() <= 128,
            "cache replan locator bound exceeded"
        );
        ensure!(
            next.origin_proofs.len() <= 128,
            "cache replan origin bound exceeded"
        );
    }
    next.global_storage_roots.sort();
    next.global_storage_roots.dedup();
    ensure!(
        next.global_storage_roots.len() <= 128,
        "cache replan root bound exceeded"
    );
    next.origin_proofs.sort_by_key(|value| {
        (
            value.config_path.clone(),
            value.server.clone(),
            value.container_path.clone(),
        )
    });
    for directory in &next.global_storage_roots {
        ensure!(
            std::time::Instant::now() < until,
            "cache replan deadline exhausted"
        );
        for fresh in origin::inspect_caches_for_replan(directory, &previous.caches)? {
            bytes = bytes
                .checked_add(fresh.executable_bytes)
                .context("cache replan byte overflow")?;
            ensure!(
                bytes <= 2 * 1024 * 1024 * 1024,
                "cache replan byte bound exceeded"
            );
            if let Some(old) = next
                .caches
                .iter()
                .find(|old| old.executable == fresh.executable)
            {
                // Replacing an accepted inode is not permission to reuse its
                // deletion/process witnesses. Preserve that conflicting path.
                ensure!(
                    *old == fresh,
                    "accepted cache identity changed; preserve conflicting image"
                );
            } else {
                next.caches.push(fresh);
            }
            ensure!(
                next.caches.len() <= 128,
                "cache replan image bound exceeded"
            );
        }
    }
    ensure!(
        std::time::Instant::now() < until,
        "cache replan deadline exhausted"
    );
    next.caches
        .sort_by(|left, right| left.executable.cmp(&right.executable));
    next.digest()?;
    Ok(next)
}

fn persist_plan(root: &Path, record: &Replan) -> Result<()> {
    let bytes = serde_json::to_vec(record)?;
    ensure!(
        bytes.len() as u64 <= MAX_REPLAN_BYTES,
        "cache replan archive exceeds bound"
    );
    let destination = root.join(name(
        &record.previous.inspection.active,
        &record.previous.inspection_sha256,
    ));
    let mut temporary = tempfile::NamedTempFile::new_in(root)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(destination)?;
    sync_directory(root)
}

pub(crate) fn reprepare(
    root: &Path,
    expected: &str,
    context: &Path,
    input: Option<&Path>,
    expected_previous: &str,
) -> Result<serde_json::Value> {
    digest(expected_previous)?;
    let root = root.canonicalize()?;
    let _lock = existing_install_lock(&root)?;
    selected(&root, expected)?;
    require_issuer_rotation_settled(&root)?;
    activation::require_settled(&root)?;
    payload_repair::require_settled(&root)?;
    lifecycle::require_settled(&root)?;
    settle_withdrawal(&root)?;
    let previous =
        read(&root, expected)?.context("cache replan requires an accepted native inspection")?;
    ensure!(
        previous.inspection.context_global_storage == origin::verified_global_storage(context)?,
        "cache replan context changed"
    );
    let archive = root.join(name(expected, expected_previous));
    if archive.try_exists()? {
        let record: Replan = serde_json::from_slice(&read_bounded(&archive, MAX_REPLAN_BYTES)?)?;
        record.validate_transition()?;
        ensure!(
            record.schema == 1
                && record.previous.inspection_sha256 == expected_previous
                && record.previous.inspection.active == expected
                && record.previous.inspection.context_global_storage
                    == previous.inspection.context_global_storage
                && record.previous.withdrawal_operation.is_none()
                && record.next.withdrawal_operation.is_none(),
            "cache replan archive binding changed"
        );
        validate(&root, expected, &record.previous)?;
        validate(&root, expected, &record.next)?;
        let current = serde_json::to_vec(&previous)?;
        if current == serde_json::to_vec(&record.next)? {
            return record.next.inspection.reply();
        }
        ensure!(
            current == serde_json::to_vec(&record.previous)?,
            "cache replan preimage changed"
        );
        // Crash recovery commits the exact already captured replacement; it
        // cannot consume new caller locators or silently widen the roster.
        save(&root, &record.next)?;
        return record.next.inspection.reply();
    }
    ensure!(
        previous.inspection_sha256 == expected_previous,
        "cache replan preimage changed"
    );
    let prefix = format!("legacy-cache-replan-{expected}-");
    let mut count = 0;
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|value| value.starts_with(&prefix))
        {
            safe_file(&entry.path())?;
            count += 1;
            ensure!(
                count < 4,
                "cache replan limit reached; preserve pending inspection"
            );
        }
    }
    let inspection = next_inspection(&previous.inspection, input)?;
    if inspection == previous.inspection {
        return previous.inspection.reply();
    }
    let next = Journal {
        schema: 1,
        inspection_sha256: inspection.digest()?,
        inspection,
        image_exclusions: previous.image_exclusions.clone(),
        withdrawal_operation: None,
    };
    validate(&root, expected, &next)?;
    let record = Replan {
        schema: 1,
        previous,
        next,
    };
    record.validate_transition()?;
    persist_plan(&root, &record)?;
    save(&root, &record.next)?;
    record.next.inspection.reply()
}
