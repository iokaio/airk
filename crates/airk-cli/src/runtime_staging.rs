// SPDX-License-Identifier: Apache-2.0
//! Durable bounded offline staging. No immutable payload or unknown file is overwritten.
use super::*;
use std::collections::BTreeSet;

const MAX_INTENT: u64 = MAX_MANIFEST * 8 + 16_384;
const PARTS: &str = ".airk-stage-parts";
fn create_private_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    let mut builder = fs::DirBuilder::new();
    #[cfg(not(unix))]
    let builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    local_ipc::prepare_root(path)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StageIntent {
    schema: u32,
    root: String,
    leaf: String,
    approval: Approval,
    manifest_json: String,
    lengths: BTreeMap<String, u64>,
}
impl StageIntent {
    fn path(root: &Path, identity: &str) -> PathBuf {
        root.join(format!("stage-{identity}.json"))
    }
    fn read(root: &Path, identity: &str) -> Result<(Self, InstallManifest)> {
        digest(identity)?;
        let value: Self =
            serde_json::from_slice(&read_bounded(&Self::path(root, identity), MAX_INTENT)?)?;
        ensure!(
            value.schema == 1
                && value.root
                    == root
                        .canonicalize()?
                        .to_str()
                        .context("non-UTF-8 installation")?
                && value.leaf.len() == 38
                && value.leaf.starts_with("stage-")
                && value.leaf[6..]
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                && value.approval.schema == 1
                && value.approval.manifest_sha256 == identity
                && value.manifest_json.len() as u64 <= MAX_MANIFEST
                && format!("{:x}", Sha256::digest(value.manifest_json.as_bytes())) == identity,
            "invalid staging ownership intent"
        );
        let inventory: InstallManifest = serde_json::from_str(&value.manifest_json)?;
        ensure!(
            inventory.schema == 1
                && (4..=MAX_FILES).contains(&inventory.files.len())
                && inventory.version == value.approval.version
                && inventory.files.len() == value.lengths.len(),
            "invalid staging inventory"
        );
        let mut bytes = 0u64;
        for (name, hash) in &inventory.files {
            payload_path(root, name)?;
            digest(hash)?;
            ensure!(!reserved(name), "reserved staging payload name");
            let length = *value.lengths.get(name).context("staging length missing")?;
            ensure!(
                length <= 2 * 1024 * 1024 * 1024,
                "oversized staging component"
            );
            bytes = bytes.checked_add(length).context("staging size overflow")?;
        }
        ensure!(bytes <= MAX_PAYLOAD, "staging payload exceeds allowance");
        Ok((value, inventory))
    }
    fn directory(&self, root: &Path) -> PathBuf {
        root.join("runtimes").join(&self.leaf)
    }
}

#[cfg(test)]
#[path = "runtime_staging_tests.rs"]
mod tests;
pub(super) fn reserved(name: &str) -> bool {
    let first = name.split('/').next().unwrap_or("").to_ascii_lowercase();
    matches!(
        first.as_str(),
        "airk-install.json" | "approval.json" | PARTS
    )
}

/// Read-only inventory for independent bootstrap diagnostics.
pub(super) fn pending(root: &Path) -> Result<Vec<String>> {
    let mut identities = Vec::new();
    for (count, entry) in fs::read_dir(root)?.enumerate() {
        ensure!(
            count < 4096,
            "installation metadata inventory exceeds bound"
        );
        let name = entry?.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if let Some(identity) = name
            .strip_prefix("stage-")
            .and_then(|name| name.strip_suffix(".json"))
        {
            digest(identity)?;
            ensure!(identities.len() < 4, "staging intent count exceeds bound");
            StageIntent::read(root, identity)?;
            identities.push(identity.to_owned());
        }
    }
    identities.sort();
    Ok(identities)
}

fn occupied_slots(root: &Path) -> Result<usize> {
    let runtimes = root.join("runtimes");
    let mut slots = fs::read_dir(&runtimes)?.take(5).count();
    ensure!(slots <= 4, "runtime directory count exceeds bound");
    for identity in pending(root)? {
        let (intent, _) = StageIntent::read(root, &identity)?;
        if !intent.directory(root).try_exists()? && !runtimes.join(identity).try_exists()? {
            slots += 1;
        }
    }
    Ok(slots)
}

fn validate(
    directory: &Path,
    intent: &StageIntent,
    inventory: &InstallManifest,
) -> Result<Vec<PathBuf>> {
    local_ipc::prepare_root(directory)?;
    let mut allowed_directories = BTreeSet::from([PathBuf::from(PARTS)]);
    for name in inventory.files.keys() {
        let mut parent = Path::new(name).parent();
        while let Some(value) = parent.filter(|value| !value.as_os_str().is_empty()) {
            allowed_directories.insert(value.to_path_buf());
            parent = value.parent();
        }
    }
    let mut stack = vec![directory.to_path_buf()];
    let mut directories = Vec::new();
    let mut count = 0;
    let mut payload_bytes = 0u64;
    let mut partials = 0;
    while let Some(parent) = stack.pop() {
        for entry in fs::read_dir(&parent)? {
            count += 1;
            ensure!(count <= 4100, "staging topology exceeds bound");
            let path = entry?.path();
            let relative = path.strip_prefix(directory)?;
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                ensure!(
                    allowed_directories.contains(relative),
                    "unknown staging directory preserved"
                );
                local_ipc::prepare_root(&path)?;
                directories.push(path.clone());
                stack.push(path);
                continue;
            }
            safe_file(&path)?;
            let name = relative
                .to_str()
                .context("invalid staging filename")?
                .replace('\\', "/");
            if name == "airk-install.json" {
                ensure!(
                    read_bounded(&path, MAX_MANIFEST)? == intent.manifest_json.as_bytes(),
                    "changed staged manifest preserved"
                );
            } else if name == "approval.json" {
                let approval: Approval = serde_json::from_slice(&read_bounded(&path, 4096)?)?;
                ensure!(
                    approval == intent.approval,
                    "changed staged approval preserved"
                );
            } else if let Some(key) = name.strip_prefix(&format!("{PARTS}/")) {
                digest(key)?;
                partials += 1;
                ensure!(partials <= 1, "staging partial-file bound exceeded");
                let maximum = inventory
                    .files
                    .iter()
                    .filter(|(_, hash)| hash.as_str() == key)
                    .filter_map(|(name, _)| intent.lengths.get(name))
                    .copied()
                    .min()
                    .or_else(|| {
                        (key == intent.approval.manifest_sha256)
                            .then_some(intent.manifest_json.len() as u64)
                    })
                    .or_else(|| {
                        let bytes = serde_json::to_vec(&intent.approval).ok()?;
                        (key == format!("{:x}", Sha256::digest(&bytes)))
                            .then_some(bytes.len() as u64)
                    })
                    .context("unknown staging partial preserved")?;
                ensure!(
                    metadata.len() <= maximum,
                    "oversized staging partial preserved"
                );
                if inventory.files.values().any(|hash| hash == key) {
                    payload_bytes = payload_bytes
                        .checked_add(metadata.len())
                        .context("staging size overflow")?;
                }
            } else {
                let expected = inventory
                    .files
                    .get(&name)
                    .context("unknown staging file preserved")?;
                ensure!(
                    metadata.len() == intent.lengths[&name]
                        && runtime_pair::digest_file(&path)? == *expected,
                    "changed staged component preserved"
                );
                payload_bytes = payload_bytes
                    .checked_add(metadata.len())
                    .context("staging size overflow")?;
            }
        }
    }
    ensure!(
        payload_bytes <= intent.lengths.values().sum::<u64>(),
        "staged data exceeds admitted payload allowance"
    );
    Ok(directories)
}

pub(super) fn abort(root: &Path, identity: &str) -> Result<()> {
    let (intent, inventory) = StageIntent::read(root, identity)?;
    let directory = intent.directory(root);
    if directory.try_exists()? {
        let mut directories = validate(&directory, &intent, &inventory)?;
        for name in inventory.files.keys() {
            let path = payload_path(&directory, name)?;
            if path.try_exists()? {
                fs::remove_file(&path)?;
                sync_directory(path.parent().unwrap())?;
            }
        }
        let parts = directory.join(PARTS);
        if parts.try_exists()? {
            for entry in fs::read_dir(&parts)? {
                fs::remove_file(entry?.path())?;
            }
            sync_directory(&parts)?;
        }
        for name in ["airk-install.json", "approval.json"] {
            let path = directory.join(name);
            if path.try_exists()? {
                fs::remove_file(path)?;
            }
        }
        directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
        for path in directories {
            fs::remove_dir(path)?;
        }
        fs::remove_dir(directory)?;
        sync_directory(&root.join("runtimes"))?;
    }
    fs::remove_file(StageIntent::path(root, identity))?;
    sync_directory(root)
}

pub(super) fn stage(
    source: &Path,
    root: &Path,
    approved: &str,
    basis: VerificationBasis,
) -> Result<Approval> {
    let _lock = install_lock(root)?;
    ensure!(
        !root.join("uninstall.json").try_exists()?,
        "resume pending runtime uninstall before staging"
    );
    let source = source.canonicalize()?;
    let (bytes, inventory) = manifest(&source, approved)?;
    let runtimes = root.join("runtimes");
    local_ipc::prepare_root(&runtimes)?;
    let destination = runtimes.join(approved);
    if destination.try_exists()? {
        let (main, approval) = verify_runtime(root, approved)?;
        if StageIntent::path(root, approved).try_exists()? {
            let (intent, _) = StageIntent::read(root, approved)?;
            ensure!(
                !intent.directory(root).try_exists()?,
                "ambiguous staging and promoted runtime preserved"
            );
            fs::remove_file(StageIntent::path(root, approved))?;
            sync_directory(root)?;
        }
        prepare_launcher(root, &main)?;
        return Ok(approval);
    }
    let intent = if StageIntent::path(root, approved).try_exists()? {
        ensure!(
            occupied_slots(root)? <= 4,
            "four retained runtimes/staging reservations occupy this installation"
        );
        let (intent, _) = StageIntent::read(root, approved)?;
        ensure!(
            intent.manifest_json.as_bytes() == bytes,
            "staging manifest changed"
        );
        intent
    } else {
        ensure!(
            occupied_slots(root)? < 4,
            "four retained runtimes/staging directories occupy this installation"
        );
        let lengths = inventory
            .files
            .keys()
            .map(|name| {
                Ok((
                    name.clone(),
                    fs::metadata(payload_path(&source, name)?)?.len(),
                ))
            })
            .collect::<Result<_>>()?;
        let intent = StageIntent {
            schema: 1,
            root: root
                .canonicalize()?
                .to_str()
                .context("non-UTF-8 installation")?
                .into(),
            leaf: format!("stage-{}", airk_kernel::service_broker::new_instance_id()?),
            approval: Approval {
                schema: 1,
                manifest_sha256: approved.into(),
                version: inventory.version.clone(),
                verification_basis: basis,
            },
            manifest_json: String::from_utf8(bytes)?,
            lengths,
        };
        let bytes = serde_json::to_vec(&intent)?;
        ensure!(
            bytes.len() as u64 <= MAX_INTENT,
            "staging intent exceeds byte bound"
        );
        let mut temporary = tempfile::NamedTempFile::new_in(root)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary.persist_noclobber(StageIntent::path(root, approved))?;
        sync_directory(root)?;
        intent
    };
    let result = copy(&source, root, &intent, &inventory, self_test);
    if result.is_err() {
        let _ = abort(root, approved);
    }
    result
}

fn copy(
    source: &Path,
    root: &Path,
    intent: &StageIntent,
    inventory: &InstallManifest,
    verify: impl FnOnce(&Path) -> Result<()>,
) -> Result<Approval> {
    let directory = intent.directory(root);
    if !directory.try_exists()? {
        create_private_directory(&directory)?;
        sync_directory(&root.join("runtimes"))?;
    }
    validate(&directory, intent, inventory)?;
    let parts = directory.join(PARTS);
    if !parts.try_exists()? {
        create_private_directory(&parts)?;
    }
    // validate() proved ownership and bounded size for every unfinished file.
    // Recopy those files; never overwrite a completed changed component.
    for entry in fs::read_dir(&parts)? {
        fs::remove_file(entry?.path())?;
    }
    sync_directory(&parts)?;
    for (name, expected) in &inventory.files {
        let to = payload_path(&directory, name)?;
        if to.try_exists()? {
            continue;
        }
        let from = payload_path(source, name)?;
        ensure!(
            fs::metadata(&from)?.len() == intent.lengths[name],
            "approved input length changed"
        );
        let partial = parts.join(expected);
        // The intent binds this reserved partial filename and byte allowance.
        if partial.try_exists()? {
            safe_file(&partial)?;
            fs::remove_file(&partial)?;
        }
        let bound = intent.lengths[name];
        airk_store::capacity::probe(&directory)?.require_growth(bound)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)?;
        let mut input = File::open(&from)?;
        let copied = std::io::copy(&mut (&mut input).take(bound), &mut output)?;
        let mut extra = [0u8; 1];
        ensure!(
            copied == bound
                && input.read(&mut extra)? == 0
                && input.metadata()?.len() == bound
                && runtime_pair::digest_file(&partial)? == *expected,
            "payload changed while staging"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                &partial,
                fs::Permissions::from_mode(if name == main_name() || name == helper_name() {
                    0o700
                } else {
                    0o600
                }),
            )?;
        }
        output.sync_all()?;
        drop(output);
        fs::create_dir_all(to.parent().context("payload parent missing")?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for parent in to
                .parent()
                .unwrap()
                .ancestors()
                .take_while(|path| *path != directory)
            {
                fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
            }
        }
        fs::rename(&partial, &to)?;
        sync_directory(to.parent().unwrap())?;
        sync_directory(&parts)?;
    }
    metadata(
        &directory,
        "airk-install.json",
        intent.manifest_json.as_bytes(),
    )?;
    metadata(
        &directory,
        "approval.json",
        &serde_json::to_vec(&intent.approval)?,
    )?;
    fs::remove_dir(&parts)?;
    verify(&directory.join(main_name()))?;
    ensure!(
        !parts.try_exists()?,
        "native self-test recreated staging files; preserving its output"
    );
    validate(&directory, intent, inventory)?;
    sync_directory(&directory)?;
    let destination = root.join("runtimes").join(&intent.approval.manifest_sha256);
    let until = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match fs::rename(&directory, &destination) {
            Ok(()) => break,
            Err(error)
                if cfg!(windows)
                    && matches!(error.raw_os_error(), Some(5 | 32 | 33))
                    && std::time::Instant::now() < until =>
            {
                std::thread::sleep(Duration::from_millis(100))
            }
            Err(error) => return Err(error.into()),
        }
    }
    sync_directory(&root.join("runtimes"))?;
    fs::remove_file(StageIntent::path(root, &intent.approval.manifest_sha256))?;
    sync_directory(root)?;
    prepare_launcher(root, &destination.join(main_name()))?;
    Ok(intent.approval.clone())
}

fn metadata(directory: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    let destination = directory.join(name);
    if destination.try_exists()? {
        return Ok(());
    }
    let partial = directory
        .join(PARTS)
        .join(format!("{:x}", Sha256::digest(bytes)));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&partial)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&partial, &destination)?;
    sync_directory(directory)?;
    sync_directory(&directory.join(PARTS))
}
