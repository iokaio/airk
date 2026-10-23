// SPDX-License-Identifier: Apache-2.0
//! Resumable, inventory-proven removal. Never recursively deletes an installation or indexes.
use super::*;
use std::collections::BTreeSet;

fn validated_inventory(root: &Path, identity: &str) -> Result<InstallManifest> {
    digest(identity)?;
    let intent = root.join(format!("retire-{identity}.json"));
    if !intent.try_exists()? {
        let (main, _) = verify_runtime(root, identity)?;
        let directory = main.parent().context("runtime parent missing")?;
        let bytes = read_bounded(&directory.join("airk-install.json"), MAX_MANIFEST)?;
        let mut temporary = tempfile::NamedTempFile::new_in(root)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary.persist_noclobber(&intent)?;
        sync_directory(root)?;
    }
    let bytes = read_bounded(&intent, MAX_MANIFEST)?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == identity,
        "runtime retirement receipt changed"
    );
    let inventory: InstallManifest = serde_json::from_slice(&bytes)?;
    ensure!(
        inventory.schema == 1 && inventory.files.len() <= MAX_FILES,
        "invalid runtime retirement receipt"
    );
    for (name, hash) in &inventory.files {
        payload_path(root, name)?;
        digest(hash)?;
    }
    Ok(inventory)
}

pub(super) fn remove(root: &Path, identity: &str) -> Result<()> {
    let inventory = validated_inventory(root, identity)?;
    let directory = root.join("runtimes").join(identity);
    if !directory.exists() {
        fs::remove_file(root.join(format!("retire-{identity}.json")))?;
        return Ok(());
    }
    local_ipc::prepare_root(&directory)?;
    let mut allowed_files: BTreeSet<_> = inventory.files.keys().cloned().collect();
    allowed_files.extend(["approval.json".into(), "airk-install.json".into()]);
    let mut directories = BTreeSet::new();
    for file in &allowed_files {
        let mut parent = Path::new(file).parent();
        while let Some(path) = parent.filter(|path| !path.as_os_str().is_empty()) {
            directories.insert(path.to_path_buf());
            parent = path.parent();
        }
    }
    let mut stack = vec![directory.clone()];
    let mut count = 0;
    while let Some(parent) = stack.pop() {
        for entry in fs::read_dir(parent)? {
            count += 1;
            ensure!(count <= 4096, "runtime removal inventory exceeds bound");
            let entry = entry?;
            let path = entry.path();
            let relative = path.strip_prefix(&directory)?;
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                ensure!(
                    directories.contains(relative),
                    "unknown directory preserved during runtime removal"
                );
                // Reparse/ownership checks precede descent, including on Windows.
                local_ipc::prepare_root(&path)?;
                stack.push(path);
            } else {
                safe_file(&path)?;
                let name = relative
                    .to_str()
                    .context("non-UTF-8 runtime entry")?
                    .replace('\\', "/");
                ensure!(
                    allowed_files.contains(&name),
                    "unknown file preserved during runtime removal"
                );
                if let Some(expected) = inventory.files.get(&name) {
                    ensure!(
                        runtime_pair::digest_file(&path)? == *expected,
                        "changed runtime file preserved during removal"
                    );
                }
            }
        }
    }
    // Executing Windows images fail before legal/metadata files are touched.
    let mut names = vec![main_name(), helper_name()];
    names.extend(
        inventory
            .files
            .keys()
            .map(String::as_str)
            .filter(|name| *name != main_name() && *name != helper_name()),
    );
    for name in names {
        let file = payload_path(&directory, name)?;
        if file.try_exists()? {
            safe_file(&file)?;
            ensure!(
                runtime_pair::digest_file(&file)? == inventory.files[name],
                "changed runtime file preserved during removal"
            );
            fs::remove_file(file)?;
        }
    }
    let mut directories: Vec<_> = directories.into_iter().collect();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for relative in directories {
        let path = directory.join(relative);
        if path.exists() {
            fs::remove_dir(path)?;
        }
    }
    for name in ["airk-install.json", "approval.json"] {
        let path = directory.join(name);
        if path.try_exists()? {
            safe_file(&path)?;
            let bytes = read_bounded(&path, MAX_MANIFEST)?;
            if name == "airk-install.json" {
                ensure!(
                    format!("{:x}", Sha256::digest(bytes)) == identity,
                    "changed runtime inventory preserved"
                );
            } else {
                let approval: Approval = serde_json::from_slice(&bytes)?;
                ensure!(
                    approval.schema == 1 && approval.manifest_sha256 == identity,
                    "foreign runtime approval preserved"
                );
            }
            fs::remove_file(path)?;
        }
    }
    fs::remove_dir(&directory)?;
    let startup = root.join(format!("startup-{identity}.json"));
    if startup.try_exists()? {
        safe_file(&startup)?;
        fs::remove_file(startup)?;
    }
    fs::remove_file(root.join(format!("retire-{identity}.json")))?;
    sync_directory(root)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct UninstallIntent {
    schema: u32,
    runtimes: Vec<String>,
}

pub(super) async fn uninstall(root: &Path) -> Result<serde_json::Value> {
    let intent_path = root.join("uninstall.json");
    let intent: UninstallIntent = if intent_path.try_exists()? {
        serde_json::from_slice(&read_bounded(&intent_path, 4096)?)?
    } else {
        let (_, main, approval) = selected(root)?;
        drain(root, &main, &approval).await?;
        let manager = crate::service_manager::ServiceManager::for_runtime(root)?;
        if manager.owned_registration_exists()? {
            manager.uninstall()?;
        }
        launcher_repair::settle_for_uninstall(root)?;
        for identity in staging::pending(root)? {
            staging::abort(root, &identity)?;
        }
        let mut runtimes = Vec::new();
        for entry in fs::read_dir(root.join("runtimes"))?.take(5) {
            let entry = entry?;
            let identity = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("unknown runtime directory preserved"))?;
            digest(&identity)?;
            verify_runtime(root, &identity)?;
            runtimes.push(identity);
        }
        ensure!(
            runtimes.len() <= 4,
            "runtime uninstall inventory exceeds bound"
        );
        let intent = UninstallIntent {
            schema: 1,
            runtimes,
        };
        replace_json(root, "uninstall.json", &intent)?;
        intent
    };
    ensure!(
        intent.schema == 1 && intent.runtimes.len() <= 4,
        "invalid runtime uninstall intent"
    );
    let _quiescence = quiescent(root)
        .context("service/project/embedding ownership must release before runtime removal")?;
    let selected = root.join("selected.json");
    if selected.try_exists()? {
        safe_file(&selected)?;
        fs::remove_file(selected)?;
    }
    let mut deferred = Vec::new();
    for identity in &intent.runtimes {
        digest(identity)?;
        if (root.join("runtimes").join(identity).exists()
            || root.join(format!("retire-{identity}.json")).exists())
            && remove(root, identity).is_err()
        {
            deferred.push(identity.clone());
        }
    }
    if !deferred.is_empty() {
        return Ok(
            serde_json::json!({"uninstalled":false,"deferred_runtimes":deferred,"resume":"runtime uninstall","project_data_preserved":true}),
        );
    }
    let launcher = launcher_path(root);
    if launcher.try_exists()? {
        verified_launcher(root)?;
        if fs::remove_file(launcher).is_err() {
            return Ok(
                serde_json::json!({"uninstalled":false,"deferred_launcher":true,"resume":"runtime uninstall","project_data_preserved":true}),
            );
        }
    }
    let receipt = root.join("launcher.json");
    if receipt.try_exists()? {
        let receipt_value: LauncherReceipt =
            serde_json::from_slice(&read_bounded(&receipt, 4096)?)?;
        ensure!(
            receipt_value.schema == 1,
            "foreign launcher receipt preserved"
        );
        digest(&receipt_value.sha256)?;
        fs::remove_file(receipt)?;
    }
    fs::remove_file(intent_path)?;
    sync_directory(root)?;
    Ok(
        serde_json::json!({"uninstalled":true,"project_data_preserved":true,"service_metadata_preserved":true}),
    )
}
