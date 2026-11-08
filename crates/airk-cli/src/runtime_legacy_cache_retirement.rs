// SPDX-License-Identifier: Apache-2.0
//! Native cache ownership snapshot before config migration, exact retirement after.
use super::legacy_launchers::{Image, image_exclusion, observe, stop_legacy};
use super::*;
use crate::runtime_legacy_origin::{
    self as origin, ConfigLocator, LegacyCacheObservation, LegacyOriginObservation,
};

const JOURNAL: &str = "legacy-cache-retirement.json";
const MAX_BYTES: u64 = MAX_MANIFEST;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Inspection {
    schema: u32,
    active: String,
    user: String,
    scope: String,
    context_global_storage: PathBuf,
    global_storage_roots: Vec<PathBuf>,
    caches: Vec<LegacyCacheObservation>,
    origin_proofs: Vec<LegacyOriginObservation>,
    origin_locators: Vec<ConfigLocator>,
}
impl Inspection {
    fn digest(&self) -> Result<String> {
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= 384 * 1024,
            "legacy cache inspection exceeds bound"
        );
        Ok(
            airk_core::identity::framed_digest(b"airk/legacy-cache-inspection/1", &[&bytes])
                .to_string(),
        )
    }
    fn reply(&self) -> Result<serde_json::Value> {
        let mut reply = serde_json::to_value(self)?;
        reply["inspection_sha256"] = self.digest()?.into();
        let (adoption, managed, classifications) = self.classifications()?;
        reply["adoption_locators"] = serde_json::to_value(adoption)?;
        reply["managed_locators"] = serde_json::to_value(managed)?;
        reply["origin_classifications"] = classifications.into();
        Ok(reply)
    }
    fn classifications(
        &self,
    ) -> Result<(
        Vec<ConfigLocator>,
        Vec<ConfigLocator>,
        Vec<serde_json::Value>,
    )> {
        let mut adoption = Vec::new();
        let mut managed = Vec::new();
        let mut records = Vec::new();
        let mut selected = std::collections::BTreeSet::new();
        for locator in &self.origin_locators {
            let matches = self
                .origin_proofs
                .iter()
                .filter(|proof| origin::matches_locator(proof, locator).unwrap_or(false))
                .collect::<Vec<_>>();
            let proof = matches
                .first()
                .context("accepted original locator lacks its exact native configuration proof")?;
            ensure!(
                matches.iter().all(|other| *other == *proof),
                "accepted original locator has conflicting native proofs"
            );
            let identity = proof.identity()?;
            let declaration = &proof.declaration;
            let kind = if declaration.role == "launch"
                || declaration.service_install_dir.is_some()
                || declaration.service_runtime_dir.is_some()
                || declaration.service_project.is_some()
            {
                "managed"
            } else if proof.standalone_scope().is_ok() && !declaration.no_default_project {
                "standalone"
            } else {
                "unsupported"
            };
            if selected.insert(identity) {
                match kind {
                    "managed" => managed.push(locator.clone()),
                    "standalone" => adoption.push(locator.clone()),
                    _ => {}
                }
            }
            let mut record = serde_json::json!({"locator":locator,"origin_sha256":identity,"kind":kind,"authority":"not_observed"});
            if kind == "unsupported" {
                record["code"] = "legacy_origin_declaration_unsupported".into();
                record["guidance"]="Preserve this original declaration and review its native selection; no index or grant was inferred.".into();
            }
            records.push(record);
        }
        Ok((adoption, managed, records))
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema: u32,
    inspection: Inspection,
    inspection_sha256: String,
    image_exclusions: Vec<Image>,
    #[serde(default)]
    withdrawal_operation: Option<String>,
}

pub(super) fn require_settled(root: &Path) -> Result<()> {
    ensure!(
        !root.join(JOURNAL).try_exists()?,
        "legacy_cache_retirement_pending: complete migration and retire-legacy-caches before another activation"
    );
    Ok(())
}
fn selected(root: &Path, expected: &str) -> Result<()> {
    digest(expected)?;
    ensure!(
        selection_record(root)?.active == expected,
        "legacy cache selected runtime changed"
    );
    let (main, _) = verify_runtime(root, expected)?;
    ensure!(
        runtime_pair::single_executable(&main)?,
        "legacy cache retirement requires HTTPS selected runtime"
    );
    Ok(())
}
fn scope(root: &Path) -> Result<&'static str> {
    let (_, channel) = layout::retained_parameters(root)?;
    let native = crate::user_paths::UserPaths::discover(channel)?;
    Ok(
        if crate::user_paths::canonical_location(&native.install)? == root.canonicalize()? {
            "native-defaults"
        } else {
            "context-only"
        },
    )
}
fn locators(path: Option<&Path>) -> Result<Vec<ConfigLocator>> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    ensure!(path.is_absolute(), "legacy locator input must be absolute");
    let parent = path.parent().context("legacy locator parent missing")?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("invalid legacy locator input")?;
    let raw = crate::http_endpoint::read_private(parent, name, 1024 * 1024)?
        .context("legacy locator input missing")?;
    let locators: Vec<ConfigLocator> = serde_json::from_slice(&raw)
        .map_err(|_| anyhow::anyhow!("invalid legacy locator metadata"))?;
    ensure!(locators.len() <= 128, "legacy locator count exceeds bound");
    Ok(locators)
}
fn capture(
    root: &Path,
    expected: &str,
    context: &Path,
    inputs: Option<&Path>,
) -> Result<Inspection> {
    let context = origin::verified_global_storage(context)?;
    let scope = scope(root)?;
    let mut roots = if scope == "native-defaults" {
        origin::native_cache_roots()?
    } else {
        Vec::new()
    };
    roots.push(context.clone());
    let until = std::time::Instant::now() + Duration::from_secs(60);
    let mut origins = Vec::new();
    let mut origin_bytes = 0u64;
    let origin_locators = locators(inputs)?;
    for locator in &origin_locators {
        ensure!(
            std::time::Instant::now() < until,
            "legacy origin inspection deadline exhausted"
        );
        let mut proof = origin::inspect_declared_cache(locator)?;
        origin_bytes = origin_bytes
            .checked_add(fs::symlink_metadata(proof.executable())?.len())
            .context("legacy origin verification size overflow")?;
        ensure!(
            origin_bytes <= 2 * 1024 * 1024 * 1024 && std::time::Instant::now() < until,
            "legacy origin inspection bound exhausted"
        );
        proof.revalidate()?;
        roots.push(origin::verified_global_storage(
            &proof.observation().global_storage,
        )?);
        origins.push(proof.observation().clone());
    }
    roots.sort();
    roots.dedup();
    ensure!(roots.len() <= 128, "legacy cache root count exceeds bound");
    origins.sort_by_key(|value| {
        (
            value.config_path.clone(),
            value.server.clone(),
            value.container_path.clone(),
        )
    });
    origins.dedup();
    let mut caches = Vec::new();
    let mut bytes = 0u64;
    for directory in &roots {
        ensure!(
            std::time::Instant::now() < until,
            "legacy cache inspection deadline exhausted"
        );
        for cache in origin::inspect_caches_at_global_storage(directory)? {
            bytes = bytes
                .checked_add(cache.executable_bytes)
                .context("legacy cache bytes overflow")?;
            ensure!(
                caches.len() < 128 && bytes <= 2 * 1024 * 1024 * 1024,
                "legacy cache whole-inspection bound exceeded"
            );
            caches.push(cache);
        }
    }
    caches.sort_by(|left, right| left.executable.cmp(&right.executable));
    let result = Inspection {
        schema: 1,
        active: expected.into(),
        user: local_ipc::current_user()?,
        scope: scope.into(),
        context_global_storage: context,
        global_storage_roots: roots,
        caches,
        origin_proofs: origins,
        origin_locators,
    };
    result.digest()?;
    Ok(result)
}
fn images(plan: &Inspection) -> Vec<Image> {
    plan.caches
        .iter()
        .map(|cache| Image {
            path: cache.executable.clone(),
            sha256: cache.executable_sha256.clone(),
        })
        .collect()
}
fn validate(root: &Path, expected: &str, journal: &Journal) -> Result<()> {
    let plan = &journal.inspection;
    ensure!(
        journal.schema == 1
            && plan.schema == 1
            && plan.active == expected
            && plan.user == local_ipc::current_user()?
            && plan.scope == scope(root)?
            && journal.inspection_sha256 == plan.digest()?,
        "legacy cache journal binding changed"
    );
    ensure!(
        plan.global_storage_roots.len() <= 128
            && plan.caches.len() <= 128
            && plan.origin_proofs.len() <= 128
            && plan.origin_locators.len() <= 128
            && journal.image_exclusions.len() <= 128,
        "legacy cache journal roster bound"
    );
    ensure!(
        plan.global_storage_roots
            .contains(&plan.context_global_storage),
        "legacy cache context absent from frozen roots"
    );
    let mut names = std::collections::BTreeSet::new();
    for cache in &plan.caches {
        ensure!(
            plan.global_storage_roots.contains(&cache.global_storage)
                && names.insert(cache.executable.clone()),
            "crossed legacy cache roster"
        );
        // This checks exact known producer path, owner, full hash/FileID and
        // ancestor identities, including an image/leaf removed by this journal.
        resume(cache, journal.withdrawal_operation.as_deref())?.revalidate()?;
    }
    let roster = images(plan);
    for (index, proof) in journal.image_exclusions.iter().enumerate() {
        ensure!(
            roster.contains(proof) && !journal.image_exclusions[..index].contains(proof),
            "crossed legacy cache image exclusion"
        );
    }
    Ok(())
}
fn read(root: &Path, expected: &str) -> Result<Option<Journal>> {
    let path = root.join(JOURNAL);
    if !path.try_exists()? {
        return Ok(None);
    }
    let journal: Journal = serde_json::from_slice(&read_bounded(&path, MAX_BYTES)?)?;
    validate(root, expected, &journal)?;
    Ok(Some(journal))
}
fn save(root: &Path, journal: &Journal) -> Result<()> {
    ensure!(
        serde_json::to_vec(journal)?.len() as u64 <= MAX_BYTES,
        "legacy cache journal size bound"
    );
    replace_json(root, JOURNAL, journal)
}
pub(super) fn inspect(
    root: &Path,
    expected: &str,
    context: &Path,
    input: Option<&Path>,
    prepare: bool,
) -> Result<serde_json::Value> {
    let canonical = root.canonicalize()?;
    let root = canonical.as_path();
    let _lock = existing_install_lock(root)?;
    selected(root, expected)?;
    if let Some(saved) = read(root, expected)? {
        ensure!(
            saved.inspection.context_global_storage == origin::verified_global_storage(context)?,
            "pending legacy cache context changed"
        );
        return saved.inspection.reply();
    }
    let plan = capture(root, expected, context, input)?;
    if prepare {
        require_issuer_rotation_settled(root)?;
        activation::require_settled(root)?;
        payload_repair::require_settled(root)?;
        save(
            root,
            &Journal {
                schema: 1,
                inspection_sha256: plan.digest()?,
                inspection: plan.clone(),
                image_exclusions: Vec::new(),
                withdrawal_operation: None,
            },
        )?;
    }
    plan.reply()
}
fn resume(
    cache: &LegacyCacheObservation,
    operation: Option<&str>,
) -> Result<origin::NativeLegacyCacheImage> {
    if let Some(operation) = operation {
        let target = origin::withdrawal_path(&cache.executable, operation)?;
        if target.try_exists()? {
            ensure!(
                !cache.executable.try_exists()?,
                "old command reappeared while its original image was withheld"
            );
            return origin::resume_withdrawn_cache(cache, operation);
        }
    }
    origin::resume_cache(cache)
}

/// This capability is created only while the caller holds installer maintenance
/// exclusion. The private journal binds every image before the first rename.
pub(super) struct CacheWithdrawal {
    root: PathBuf,
    expected: String,
    operation: String,
    journal: Journal,
    _exclusion: legacy_launchers::ImageExclusion,
    observation: serde_json::Value,
}
impl CacheWithdrawal {
    pub(super) fn revalidate_identity(&self) -> Result<()> {
        let saved: Journal =
            serde_json::from_slice(&read_bounded(&self.root.join(JOURNAL), MAX_BYTES)?)?;
        ensure!(
            serde_json::to_vec(&saved)? == serde_json::to_vec(&self.journal)?,
            "native cache withdrawal journal changed during archive copy"
        );
        ensure!(
            selection_record(&self.root)?.active == self.expected,
            "native selected runtime changed during archive copy"
        );
        for cache in &self.journal.inspection.caches {
            origin::revalidate_withdrawn_cache_identity(cache, &self.operation)?;
        }
        Ok(())
    }
    pub(super) fn origin(&self, locator: &ConfigLocator) -> Result<LegacyOriginObservation> {
        let mut matches = self
            .journal
            .inspection
            .origin_proofs
            .iter()
            .filter(|proof| origin::matches_locator(proof, locator).unwrap_or(false));
        let origin = matches
            .next()
            .context("original declaration was not part of accepted native cache preparation")?;
        ensure!(
            matches.next().is_none(),
            "ambiguous original declaration in native cache preparation"
        );
        Ok(origin.clone())
    }
    pub(super) fn revalidate(&mut self) -> Result<()> {
        let saved =
            read(&self.root, &self.expected)?.context("native cache preparation disappeared")?;
        ensure!(
            saved.inspection_sha256 == self.journal.inspection_sha256
                && saved.withdrawal_operation.as_deref() == Some(self.operation.as_str()),
            "native cache withdrawal binding changed"
        );
        for cache in &saved.inspection.caches {
            let mut held = origin::resume_withdrawn_cache(cache, &self.operation)?;
            held.revalidate()?;
            ensure!(held.exists(), "withheld legacy image disappeared");
        }
        Ok(())
    }
    pub(super) fn observation(&self) -> Result<serde_json::Value> {
        Ok(self.observation.clone())
    }
}
pub(super) fn withhold_locked(
    root: &Path,
    expected: &str,
    inspection: &str,
    operation: &str,
) -> Result<CacheWithdrawal> {
    selected(root, expected)?;
    let mut journal = read(root, expected)?
        .context("prepare original cache ownership before saved-index adoption")?;
    ensure!(
        journal.inspection_sha256 == inspection,
        "saved-index cache preparation preimage changed"
    );
    if let Some(previous) = &journal.withdrawal_operation {
        ensure!(
            previous == operation,
            "another cache withdrawal operation is pending"
        );
    } else {
        // Validate the exact operation before its intent is persisted.
        if let Some(cache) = journal.inspection.caches.first() {
            origin::withdrawal_path(&cache.executable, operation)?;
        }
        journal.withdrawal_operation = Some(operation.into());
        save(root, &journal)?;
    }
    let original = images(&journal.inspection);
    #[cfg(windows)]
    let original_paths: Vec<_> = original.iter().map(|image| image.path.clone()).collect();
    // Windows image write exclusion prevents a new execution while each image
    // is moved. Unix exclusive rename withdraws configured names first; exact
    // process census plus historical writer/admission proof is still required.
    #[cfg(windows)]
    stop_legacy(&original_paths)?;
    #[cfg(windows)]
    let exclusion = image_exclusion(&original)?;
    for cache in &journal.inspection.caches {
        let target = origin::withdrawal_path(&cache.executable, operation)?;
        if !target.try_exists()? {
            resume(cache, Some(operation))?.withdraw(operation)?;
        }
        origin::resume_withdrawn_cache(cache, operation)?.revalidate()?;
    }
    let mut roster = original.clone();
    for cache in &journal.inspection.caches {
        roster.push(Image {
            path: origin::withdrawal_path(&cache.executable, operation)?,
            sha256: cache.executable_sha256.clone(),
        });
    }
    let paths: Vec<_> = roster.iter().map(|image| image.path.clone()).collect();
    stop_legacy(&paths)?;
    #[cfg(not(windows))]
    let exclusion = image_exclusion(&roster)?;
    let mut proofs = journal.image_exclusions.clone();
    proofs.extend(exclusion.images.clone());
    #[cfg(windows)]
    for image in &original {
        proofs.push(Image {
            path: origin::withdrawal_path(&image.path, operation)?,
            sha256: image.sha256.clone(),
        });
    }
    proofs.sort_by(|left, right| left.path.cmp(&right.path));
    proofs.dedup();
    let observations = observe(&roster, &proofs)?;
    let observation = serde_json::json!({"schema":1,"inspection_sha256":inspection,"operation_id":operation,
        "scope":"verified_legacy_cache_images","image_roster":roster,"image_exclusions":proofs,"remaining":0,"observations":observations});
    let mut cap = CacheWithdrawal {
        root: root.into(),
        expected: expected.into(),
        operation: operation.into(),
        journal,
        _exclusion: exclusion,
        observation,
    };
    cap.revalidate()?;
    Ok(cap)
}
pub(super) fn restore_locked(mut cap: CacheWithdrawal) -> Result<serde_json::Value> {
    cap.revalidate()?;
    for cache in &cap.journal.inspection.caches {
        origin::resume_withdrawn_cache(cache, &cap.operation)?.restore_withdrawn(&cap.operation)?;
    }
    cap.journal.withdrawal_operation = None;
    save(&cap.root, &cap.journal)?;
    Ok(
        serde_json::json!({"schema":1,"operation_id":cap.operation,"inspection_sha256":cap.journal.inspection_sha256,"original_image_paths_restored":true,"retirement_pending":true}),
    )
}

pub(super) fn retire(
    root: &Path,
    expected: &str,
    inspection_sha: &str,
) -> Result<serde_json::Value> {
    let canonical = root.canonicalize()?;
    let root = canonical.as_path();
    let _lock = existing_install_lock(root)?;
    selected(root, expected)?;
    require_issuer_rotation_settled(root)?;
    activation::require_settled(root)?;
    payload_repair::require_settled(root)?;
    lifecycle::require_settled(root)?;
    let mut journal = read(root, expected)?
        .context("prepare the actual legacy cache inspection before migration")?;
    ensure!(
        journal.withdrawal_operation.is_none(),
        "complete pending saved-index adoption before retiring legacy caches"
    );
    let (_, _, classifications) = journal.inspection.classifications()?;
    ensure!(
        classifications
            .iter()
            .all(|value| value["kind"] != "unsupported"),
        "unsupported original cached declaration requires native selection review before cleanup"
    );
    ensure!(
        journal.inspection_sha256 == inspection_sha,
        "legacy cache inspection preimage changed"
    );
    let roster = images(&journal.inspection);
    let paths: Vec<_> = roster.iter().map(|image| image.path.clone()).collect();
    let configuration_migration =
        origin::require_migrated_origins(&journal.inspection.origin_proofs, &paths)?;
    let current_exe = std::env::current_exe()?.canonicalize()?;
    ensure!(
        paths
            .iter()
            .all(|path| path.canonicalize().map_or(true, |path| path != current_exe)),
        "run cache retirement from the selected modern distribution, not a cache being retired"
    );
    stop_legacy(&paths)?;
    let exclusion = image_exclusion(&roster)?;
    for proof in &exclusion.images {
        if !journal.image_exclusions.contains(proof) {
            journal.image_exclusions.push(proof.clone());
        }
    }
    save(root, &journal)?;
    let before = observe(&roster, &journal.image_exclusions)?;
    for cached in &journal.inspection.caches {
        let mut proof = origin::resume_cache(cached)?;
        proof.revalidate()?;
        if proof.exists() {
            fs::remove_file(&cached.executable)?;
            sync_directory(&cached.cache_directory)?;
        }
    }
    let observations = observe(&roster, &journal.image_exclusions)?;
    drop(exclusion);
    for cached in &journal.inspection.caches {
        let proof = origin::resume_cache(cached)?;
        drop(proof);
        if cached.cache_directory.try_exists()? {
            ensure!(
                fs::read_dir(&cached.cache_directory)?.next().is_none(),
                "unknown legacy cache leaf contents preserved"
            );
            fs::remove_dir(&cached.cache_directory)?;
            sync_directory(
                cached
                    .cache_directory
                    .parent()
                    .context("cache parent missing")?,
            )?;
        }
    }
    for directory in &journal.inspection.global_storage_roots {
        ensure!(
            origin::inspect_caches_at_global_storage(directory)?.is_empty(),
            "new legacy cache appeared outside the accepted inspection; complete configuration migration and review again"
        );
    }
    if journal.inspection.scope == "native-defaults" {
        for directory in origin::native_cache_roots()? {
            ensure!(
                origin::inspect_caches_at_global_storage(&directory)?.is_empty(),
                "new native profile cache appeared after inspection"
            );
        }
    }
    ensure!(
        origin::require_migrated_origins(&journal.inspection.origin_proofs, &paths)?
            == configuration_migration,
        "saved original configuration changed during native cache retirement; cleanup remains pending"
    );
    let plan = &journal.inspection;
    let result = serde_json::json!({"schema":1,"active":expected,"scope":plan.scope,"global_storage_roots":plan.global_storage_roots,"inspection_sha256":inspection_sha,
        "caches_retired":true,"retired_caches":plan.caches,"remaining":0,"configuration_migration":configuration_migration,"processes":{"scope":"verified_legacy_cache_images","remaining":0,
        "observations":observations,"pre_removal_observations":before,"image_roster":roster,"image_exclusions":journal.image_exclusions}});
    fs::remove_file(root.join(JOURNAL))?;
    sync_directory(root)?;
    Ok(result)
}

#[cfg(test)]
#[path = "runtime_legacy_cache_retirement_tests.rs"]
mod tests;

#[path = "runtime_legacy_cache_reprepare.rs"]
mod reprepare;
pub(super) use reprepare::reprepare;
