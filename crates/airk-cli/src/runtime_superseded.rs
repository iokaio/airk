// SPDX-License-Identifier: Apache-2.0
//! Bounded, resumable cleanup of every approved nonselected payload after migration.
use super::super::legacy_launchers::{Image, image_exclusion, observe, stop_legacy};
use super::*;

const JOURNAL: &str = "superseded-retirement.json";
const LIMIT: u64 = 4 * MAX_MANIFEST + 256 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    schema: u32,
    install: PathBuf,
    active: String,
    previous: Option<String>,
    runtimes: Vec<String>,
    manifests: BTreeMap<String, String>,
    images: Vec<Image>,
    image_exclusions: Vec<Image>,
}

pub(super) fn require_settled(root: &Path) -> Result<()> {
    ensure!(
        !present(&root.join(JOURNAL))?,
        "superseded_runtime_cleanup_pending: resume retire-previous before another activation"
    );
    Ok(())
}

pub(super) fn pending(root: &Path) -> Result<bool> {
    present(&root.join(JOURNAL))
}

fn all_identities(root: &Path, active: &str) -> Result<Vec<String>> {
    let mut identities = BTreeSet::new();
    let directory = root.join("runtimes");
    for (count, entry) in fs::read_dir(&directory)?.take(6).enumerate() {
        ensure!(count < 5, "runtime cleanup directory count exceeds bound");
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("invalid runtime cleanup directory")?;
        digest(name)?;
        local_ipc::require_existing_root(&path)?;
        if name != active {
            identities.insert(name.to_owned());
        }
    }
    // Include partially removed payloads whose independently sealed manifest
    // survived an interruption even after their last directory entry vanished.
    for (count, entry) in fs::read_dir(root)?.enumerate() {
        ensure!(count < 4096, "runtime cleanup metadata count exceeds bound");
        let name = entry?.file_name();
        if let Some(identity) = name
            .to_str()
            .and_then(|name| name.strip_prefix("retire-"))
            .and_then(|name| name.strip_suffix(".json"))
        {
            digest(identity)?;
            ensure!(
                identity != active,
                "active runtime has a retirement receipt"
            );
            identities.insert(identity.into());
        }
    }
    ensure!(
        identities.len() <= 4,
        "superseded runtime count exceeds bound"
    );
    Ok(identities.into_iter().collect())
}

fn identities(root: &Path, active: &str) -> Result<Vec<String>> {
    let preserved = activation::rebuild::preserved_runtime_ids(root)?;
    Ok(all_identities(root, active)?
        .into_iter()
        .filter(|identity| !preserved.contains(identity))
        .collect())
}

fn images_for(root: &Path, identity: &str, inventory: &InstallManifest) -> Result<Vec<Image>> {
    let mut files = inventory.files.clone();
    crate::process_roles::inventory(&mut files);
    let mut images = Vec::new();
    for name in [
        main_name(),
        helper_name(),
        MAC_LAUNCHER_IMAGE,
        crate::process_roles::Role::Service.name(),
        crate::process_roles::Role::Index.name(),
        crate::process_roles::Role::Source.name(),
    ] {
        if let Some(hash) = files.get(name) {
            let path = root.join("runtimes").join(identity).join(name);
            if !images.iter().any(|image: &Image| image.path == path) {
                images.push(Image {
                    path,
                    sha256: hash.clone(),
                });
            }
        }
    }
    Ok(images)
}

/// A synthesized process-role alias is not a sealed distribution file. If it
/// was never materialized, no Windows image handle exists from which retirement
/// could later establish exclusion. Discover that limitation before reset.
pub(super) fn absent_optional_aliases(root: &Path, active: &str) -> Result<Vec<Image>> {
    inspect_images(root, active)?;
    let mut absent = Vec::new();
    for identity in all_identities(root, active)? {
        let inventory = inspection_inventory(root, &identity)?;
        let mut possible = inventory.files.clone();
        crate::process_roles::inventory(&mut possible);
        for role in [
            crate::process_roles::Role::Service,
            crate::process_roles::Role::Index,
            crate::process_roles::Role::Source,
        ] {
            if inventory.files.contains_key(role.name()) {
                continue; // A sealed image is never an optional alias.
            }
            if let Some(sha256) = possible.get(role.name()) {
                let path = root.join("runtimes").join(&identity).join(role.name());
                if !present(&path)? {
                    absent.push(Image {
                        path,
                        sha256: sha256.clone(),
                    });
                }
            }
        }
    }
    Ok(absent)
}

fn inspection_inventory(root: &Path, identity: &str) -> Result<InstallManifest> {
    let receipt = root.join(format!("retire-{identity}.json"));
    if present(&receipt)? {
        let raw = read_bounded(&receipt, MAX_MANIFEST)?;
        ensure!(
            format!("{:x}", Sha256::digest(&raw)) == identity,
            "changed retained retirement manifest"
        );
        let inventory: InstallManifest = serde_json::from_slice(&raw)?;
        ensure!(inventory.schema == 1, "invalid retirement manifest schema");
        Ok(inventory)
    } else {
        Ok(owned_inventory(root, identity)?.2)
    }
}

pub(super) fn inspect_images(root: &Path, active: &str) -> Result<Vec<serde_json::Value>> {
    let mut result = Vec::new();
    for identity in all_identities(root, active)? {
        let preserved = activation::rebuild::preserved_runtime(root, &identity)?;
        let inventory = inspection_inventory(root, &identity)?;
        for image in images_for(root, &identity, &inventory)? {
            if present(&image.path)? {
                safe_file(&image.path)?;
                ensure!(
                    runtime_pair::digest_file(&image.path)? == image.sha256,
                    "changed superseded image preserved"
                );
                result.push(serde_json::json!({"path":image.path,"sha256":image.sha256,"runtime_sha256":identity,
                    "preserved_rebuild_evidence":preserved}));
            }
        }
    }
    Ok(result)
}

fn validate(root: &Path, intent: &Intent, expected: &str) -> Result<()> {
    let selected = selection_record(root)?;
    ensure!(
        intent.schema == 1
            && intent.install == root.canonicalize()?
            && intent.active == expected
            && selected.active == expected
            && selected
                .previous
                .as_ref()
                .is_none_or(|prior| Some(prior) == intent.previous.as_ref()),
        "superseded runtime selection or transaction changed"
    );
    ensure!(
        intent.runtimes.len() <= 4
            && intent.images.len() <= 20
            && intent.image_exclusions.len() <= 20,
        "superseded retirement roster exceeds bound"
    );
    for (index, identity) in intent.runtimes.iter().enumerate() {
        digest(identity)?;
        ensure!(
            identity != expected
                && !intent.runtimes[..index].contains(identity)
                && !activation::rebuild::preserved_runtime(root, identity)?,
            "invalid superseded runtime roster"
        );
        if present(&root.join("runtimes").join(identity))?
            || present(&root.join(format!("retire-{identity}.json")))?
        {
            ensure!(
                intent.manifests.contains_key(identity),
                "superseded runtime lacks its frozen manifest"
            );
        }
    }
    ensure!(
        intent.manifests.keys().cloned().collect::<Vec<_>>() == intent.runtimes,
        "incomplete superseded manifest roster"
    );
    let mut allowed = BTreeMap::new();
    let mut canonical_images = Vec::new();
    for (identity, raw) in &intent.manifests {
        ensure!(
            intent.runtimes.contains(identity)
                && raw.len() as u64 <= MAX_MANIFEST
                && format!("{:x}", Sha256::digest(raw.as_bytes())) == *identity,
            "crossed superseded manifest"
        );
        let mut manifest: InstallManifest = serde_json::from_str(raw)?;
        ensure!(manifest.schema == 1, "invalid superseded manifest schema");
        canonical_images.extend(images_for(root, identity, &manifest)?);
        crate::process_roles::inventory(&mut manifest.files);
        for (name, hash) in manifest.files {
            payload_path(root, &name)?;
            digest(&hash)?;
            allowed.insert(root.join("runtimes").join(identity).join(name), hash);
        }
    }
    ensure!(
        intent.images == canonical_images,
        "incomplete or crossed superseded image roster"
    );
    for (index, image) in intent.images.iter().enumerate() {
        digest(&image.sha256)?;
        ensure!(
            !intent.images[..index]
                .iter()
                .any(|prior| prior.path == image.path)
                && allowed.get(&image.path) == Some(&image.sha256)
                && intent.runtimes.iter().any(|identity| [
                    main_name(),
                    helper_name(),
                    MAC_LAUNCHER_IMAGE,
                    crate::process_roles::Role::Service.name(),
                    crate::process_roles::Role::Index.name(),
                    crate::process_roles::Role::Source.name()
                ]
                .iter()
                .any(|name| image.path == root.join("runtimes").join(identity).join(name))),
            "crossed superseded image roster"
        );
    }
    for (index, witness) in intent.image_exclusions.iter().enumerate() {
        ensure!(
            intent.images.contains(witness) && !intent.image_exclusions[..index].contains(witness),
            "crossed superseded image exclusion"
        );
    }
    Ok(())
}

fn save(root: &Path, intent: &Intent) -> Result<()> {
    validate(root, intent, &intent.active)?;
    ensure!(
        serde_json::to_vec(intent)?.len() as u64 <= LIMIT,
        "superseded retirement metadata exceeds bound"
    );
    replace_json(root, JOURNAL, intent)
}

pub(super) fn finish_for_uninstall(root: &Path, retired: &[String]) -> Result<()> {
    let path = root.join(JOURNAL);
    if !present(&path)? {
        return Ok(());
    }
    let intent: Intent = serde_json::from_slice(&read_bounded(&path, LIMIT)?)?;
    ensure!(
        intent.schema == 1
            && intent.install == root.canonicalize()?
            && retired.contains(&intent.active),
        "foreign superseded cleanup receipt preserved during uninstall"
    );
    for identity in &intent.runtimes {
        digest(identity)?;
        ensure!(
            !present(&root.join("runtimes").join(identity))?
                && !present(&root.join(format!("retire-{identity}.json")))?,
            "superseded uninstall cleanup incomplete"
        );
    }
    fs::remove_file(path)?;
    sync_directory(root)
}

fn prepare(root: &Path, expected: &str) -> Result<Intent> {
    let path = root.join(JOURNAL);
    if present(&path)? {
        let intent: Intent = serde_json::from_slice(&read_bounded(&path, LIMIT)?)?;
        validate(root, &intent, expected)?;
        ensure!(
            identities(root, expected)?
                .iter()
                .all(|identity| intent.runtimes.contains(identity)),
            "new superseded payload appeared during cleanup"
        );
        return Ok(intent);
    }
    let selected = selection_record(root)?;
    ensure!(
        selected.active == expected,
        "runtime selection changed before cleanup"
    );
    let mut runtimes = identities(root, expected)?;
    if let Some(previous) = &selected.previous {
        ensure!(
            runtimes.contains(previous),
            "selected rollback payload disappeared before cleanup"
        );
    }
    if present(&root.join(PREVIOUS_RETIREMENT))? {
        let old =
            previous_intent(root, expected)?.context("previous cleanup receipt disappeared")?;
        if !runtimes.contains(&old.previous) {
            runtimes.push(old.previous);
            runtimes.sort();
        }
    }
    let mut images = Vec::new();
    let mut manifests = BTreeMap::new();
    for identity in &runtimes {
        let inventory = validated_inventory(root,identity).context("superseded ownership manifest unavailable; restore the authenticated original inventory before resuming cleanup")?;
        let raw = String::from_utf8(read_bounded(
            &root.join(format!("retire-{identity}.json")),
            MAX_MANIFEST,
        )?)?;
        manifests.insert(identity.clone(), raw);
        images.extend(images_for(root, identity, &inventory)?);
    }
    let intent = Intent {
        schema: 1,
        install: root.canonicalize()?,
        active: expected.into(),
        previous: selected.previous,
        runtimes,
        manifests,
        images,
        image_exclusions: Vec::new(),
    };
    save(root, &intent)?;
    Ok(intent)
}

pub(super) fn retire(root: &Path, expected: &str) -> Result<serde_json::Value> {
    retire_recording(root, expected, &mut |_| Ok(()))
}

pub(super) fn retire_recording(
    root: &Path,
    expected: &str,
    record: &mut dyn FnMut(&serde_json::Value) -> Result<()>,
) -> Result<serde_json::Value> {
    legacy_launchers::require_absent(root)?;
    ensure!(
        staging::pending(root)?.is_empty(),
        "pending staging must settle before superseded cleanup"
    );
    if let Some(path) = payload_repair::preserved(root)? {
        return Ok(
            serde_json::json!({"active":expected,"complete":false,"status":"blocked","code":"preserved_damaged_payload_requires_review",
            "preserved_damaged_payload":{"path":path,"status":"preserved_modified_or_unknown"},
            "guidance":"The reviewed repair preserved modified or unknown original files. Review that quarantine explicitly before declaring obsolete executable removal complete; automatic cleanup will not delete it."}),
        );
    }
    if present(&root.join(PREVIOUS_RETIREMENT))? {
        let old =
            previous_intent(root, expected)?.context("previous cleanup receipt disappeared")?;
        if !present(&root.join("runtimes").join(&old.previous))?
            && !present(&root.join(format!("retire-{}.json", old.previous)))?
        {
            return Ok(
                serde_json::json!({"active":expected,"complete":false,"status":"blocked","code":"superseded_ownership_manifest_unavailable","missing_runtime":old.previous,
                "guidance":"An older interrupted cleanup lost both its payload and original ownership inventory. Restore the authenticated original inventory or distribution before retrying; no process identity or empty roster is invented."}),
            );
        }
    }
    let mut intent = prepare(root, expected)?;
    validate(root, &intent, expected)?;
    for identity in &intent.runtimes {
        if present(&root.join("runtimes").join(identity))?
            || present(&root.join(format!("retire-{identity}.json")))?
        {
            preflight_remove(root, identity)?;
        }
    }
    // Recheck every extant process image against the original native-approved
    // inventory before any signal; PID or directory names alone grant nothing.
    for image in &intent.images {
        if present(&image.path)? {
            safe_file(&image.path)?;
            ensure!(
                runtime_pair::digest_file(&image.path)? == image.sha256,
                "changed obsolete image preserved"
            );
        }
    }
    let paths: Vec<_> = intent
        .images
        .iter()
        .map(|image| image.path.clone())
        .collect();
    stop_legacy(&paths)?;
    let exclusion = image_exclusion(&intent.images)?;
    for image in &exclusion.images {
        if !intent.image_exclusions.contains(image) {
            intent.image_exclusions.push(image.clone());
        }
    }
    save(root, &intent)?;
    let pre_observations = observe(&intent.images, &intent.image_exclusions)?;
    // Remove image names under the held Windows write exclusion, then close the
    // handles before directory removal. No modern selected image is in this list.
    for image in &intent.images {
        if present(&image.path)? {
            safe_file(&image.path)?;
            ensure!(
                runtime_pair::digest_file(&image.path)? == image.sha256,
                "changed obsolete image preserved"
            );
            fs::remove_file(&image.path)?;
            sync_directory(
                image
                    .path
                    .parent()
                    .context("obsolete image parent missing")?,
            )?;
        }
    }
    let observations = observe(&intent.images, &intent.image_exclusions)?;
    drop(exclusion);
    finish_payloads(root, &intent)?;
    let result = serde_json::json!({"active":expected,"retired_previous":intent.previous,"retired_runtimes":intent.runtimes,
        "preserved_rebuild_runtimes":activation::rebuild::preserved_runtime_ids(root)?,"complete":true,
        "processes":{"scope":"verified_superseded_runtime_images","remaining":0,"observations":observations,"pre_removal_observations":pre_observations,"image_roster":intent.images,"image_exclusions":intent.image_exclusions},"selected_service_restarted":false});
    record(&result)?;
    fs::remove_file(root.join(JOURNAL))?;
    sync_directory(root)?;
    Ok(result)
}

fn finish_payloads(root: &Path, intent: &Intent) -> Result<()> {
    validate(root, intent, &intent.active)?;
    let expected = &intent.active;
    replace_json(
        root,
        "selected.json",
        &Selection {
            schema: 1,
            active: expected.clone(),
            previous: None,
        },
    )?;
    for identity in &intent.runtimes {
        if present(&root.join("runtimes").join(identity))?
            || present(&root.join(format!("retire-{identity}.json")))?
        {
            remove(root, identity)?;
        }
    }
    ensure!(
        identities(root, expected)?.is_empty(),
        "superseded cleanup left old runtime state"
    );
    if present(&root.join(PREVIOUS_RETIREMENT))? {
        previous_intent(root, expected)?;
        fs::remove_file(root.join(PREVIOUS_RETIREMENT))?;
    }
    sync_directory(root)
}

#[cfg(test)]
#[path = "runtime_superseded_tests.rs"]
mod tests;
