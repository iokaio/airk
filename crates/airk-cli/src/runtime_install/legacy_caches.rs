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
    let native = crate::runtime_install::discover_user_paths(channel)?;
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
    ensure!(
        journal.withdrawal_operation.is_none(),
        "settle the interrupted saved-index cache withdrawal before using this journal"
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
        origin::resume_cache(cache)?.revalidate()?;
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
    settle_withdrawal(root)?;
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
/// Simplification S2: standalone adoption is no longer offered and an
/// interrupted one is never resumed. Its cache withdrawal is corrected where
/// possible: a withheld image whose original path is empty is moved back with
/// the exact native identity checks; a withheld duplicate beside a present
/// original is removed. The journal then records no withdrawal. The caller
/// holds the installer lock. Repeating this after a crash at any point is safe:
/// each image is handled from its current files, and the journal changes last.
fn settle_withdrawal(root: &Path) -> Result<()> {
    let path = root.join(JOURNAL);
    if !path.try_exists()? {
        return Ok(());
    }
    let mut journal: Journal = serde_json::from_slice(&read_bounded(&path, MAX_BYTES)?)?;
    let Some(operation) = journal.withdrawal_operation.clone() else {
        return Ok(());
    };
    let plan = &journal.inspection;
    ensure!(
        journal.schema == 1
            && plan.schema == 1
            && plan.user == local_ipc::current_user()?
            && journal.inspection_sha256 == plan.digest()?
            && plan.caches.len() <= 128,
        "legacy cache journal binding changed; interrupted cache withdrawal preserved"
    );
    let mut settlement = legacy_maintenance::Settlement::default();
    for cache in &plan.caches {
        let withheld = origin::withdrawal_path(&cache.executable, &operation)?;
        ensure!(
            withheld.parent() == Some(cache.cache_directory.as_path()),
            "withheld legacy image is outside its accepted cache directory"
        );
        match (present(&withheld)?, present(&cache.executable)?) {
            (true, false) => {
                origin::resume_withdrawn_cache(cache, &operation)?.restore_withdrawn(&operation)?;
                settlement.restored(
                    &cache.executable,
                    "withheld legacy helper image moved back to its original path",
                );
            }
            (true, true) => {
                safe_file(&withheld)?;
                fs::remove_file(&withheld)?;
                sync_directory(&cache.cache_directory)?;
                settlement.deleted(
                    &withheld,
                    "withheld duplicate removed because the original image is present",
                );
            }
            (false, _) => {}
        }
    }
    journal.withdrawal_operation = None;
    save(root, &journal)?;
    settlement.restored(
        &path,
        &format!(
            "cleared the interrupted saved-index cache withdrawal {operation}; the accepted cache inventory is kept"
        ),
    );
    settlement.report();
    Ok(())
}

fn present(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
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
    settle_withdrawal(root)?;
    let mut journal = read(root, expected)?
        .context("prepare the actual legacy cache inspection before migration")?;
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
mod tests;

mod reprepare;
pub(super) use reprepare::reprepare;
