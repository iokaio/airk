// SPDX-License-Identifier: Apache-2.0
//! Native portable setup through the same operation and ownership transaction as installers.
use crate::{
    contract::{ErrorLine, ExitClass, SetupReceiptObservation},
    install,
};
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    install_record::{
        Acknowledgements, IndexDeletion, Locations, MAX_SETUP_OPERATION_LIFETIME_MS, OperationKind,
        SETUP_OPERATION_SCHEMA, SetupOperation, SetupOptions, TrustChoice, canonical,
        payload_inventory::{INVENTORY_FILE, MAX_INVENTORY_BYTES, PayloadInventory},
    },
    runtime_metadata::{MAX_RUNTIME_METADATA_BYTES, SplitRuntimeMetadata},
};
use airk_process::directory::OpenedDirectory;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

pub(crate) struct Options {
    pub program: Option<PathBuf>,
    pub resume: Option<PathBuf>,
    pub data: Option<PathBuf>,
    pub state: Option<PathBuf>,
    pub cache: Option<PathBuf>,
    pub logs: Option<PathBuf>,
}

fn digest(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}
fn bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    airk_ipc::private_record::require_regular_unlinked(path)?;
    let mut bytes = Vec::new();
    File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= limit,
        "portable metadata exceeds its byte bound"
    );
    Ok(bytes)
}
fn file_digest(path: &Path, expected: u64) -> Result<Sha256Digest> {
    airk_ipc::private_record::require_regular_unlinked(path)?;
    let parent = OpenedDirectory::open(path.parent().context("payload parent missing")?)?;
    let file = parent.open_file(path.file_name().context("payload filename missing")?)?;
    #[cfg(windows)]
    airk_process::directory::require_default_stream(path, &file)?;
    ensure!(
        file.metadata()?.len() == expected,
        "portable payload changed size"
    );
    let mut hash = Sha256::new();
    let read = std::io::copy(&mut file.take(expected.saturating_add(1)), &mut hash)?;
    ensure!(read == expected, "portable payload changed while hashing");
    Ok(Sha256Digest::from_bytes(hash.finalize().into()))
}

fn locations(
    options: &Options,
    existing: Option<&airk_core::install_record::setup_recovery::InstallationImage>,
) -> Result<Locations> {
    #[cfg(unix)]
    let home = airk_process::user_folders::home()?;
    #[cfg(windows)]
    let base = airk_process::user_folders::known_folder(
        airk_process::user_folders::KnownFolder::LocalAppData,
    )?
    .join("AIRK Service");
    #[cfg(target_os = "macos")]
    let base = home.join("Library/Application Support/AIRK");
    #[cfg(all(unix, not(target_os = "macos")))]
    let base = home.join(".local/share/airk");
    let current = existing.map(|existing| &existing.record.locations);
    let path =
        |provided: &Option<PathBuf>, old: Option<&String>, fallback: PathBuf| -> Result<String> {
            let value = provided
                .clone()
                .or_else(|| old.map(PathBuf::from))
                .unwrap_or(fallback);
            Ok(airk_process::user_folders::checked(value)?
                .to_str()
                .context("portable location is not UTF-8")?
                .to_owned())
        };
    #[cfg(windows)]
    let defaults = (
        base.join("data/indexes"),
        base.join("state/service"),
        base.join("cache/service"),
        base.join("logs/service"),
    );
    #[cfg(target_os = "macos")]
    let defaults = (
        base.join("data/indexes"),
        base.join("state/service"),
        home.join("Library/Caches/AIRK/service"),
        home.join("Library/Logs/AIRK/service"),
    );
    #[cfg(all(unix, not(target_os = "macos")))]
    let defaults = (
        base.join("indexes"),
        home.join(".local/state/airk/service"),
        home.join(".cache/airk/service"),
        home.join(".local/state/airk/service-logs"),
    );
    Ok(Locations {
        program: airk_process::user_folders::checked(
            options.program.clone().context("--program is required")?,
        )?
        .to_str()
        .context("program path is not UTF-8")?
        .into(),
        data: path(&options.data, current.map(|old| &old.data), defaults.0)?,
        state: path(&options.state, current.map(|old| &old.state), defaults.1)?,
        cache: path(&options.cache, current.map(|old| &old.cache), defaults.2)?,
        logs: path(&options.logs, current.map(|old| &old.logs), defaults.3)?,
    })
}

fn operation(
    locations: Locations,
    id: Incarnation,
    inventory: Sha256Digest,
    now: u64,
) -> SetupOperation {
    SetupOperation {
        schema: SETUP_OPERATION_SCHEMA.into(),
        operation_id: id,
        kind: OperationKind::Install,
        created_unix_ms: now,
        expires_unix_ms: now + MAX_SETUP_OPERATION_LIFETIME_MS,
        requested_by: "airkctl-portable".into(),
        acknowledgements: Acknowledgements {
            predecessor_removal: false,
            index_reset: false,
        },
        locations,
        payload_inventory_sha256: inventory,
        options: SetupOptions {
            login_startup: false,
            trust: TrustChoice::Skip,
            start: false,
            delete_indexes: IndexDeletion::Keep,
        },
    }
}

fn names(
    root: &Path,
    prefix: &str,
    found: &mut BTreeSet<String>,
    budget: &mut usize,
) -> Result<()> {
    for entry in std::fs::read_dir(root)? {
        ensure!(*budget > 0, "portable runtime has too many entries");
        *budget -= 1;
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("portable file name is not UTF-8"))?;
        let relative = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let metadata = std::fs::symlink_metadata(entry.path())?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "portable runtime contains a link"
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes()
                    & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
                    == 0,
                "portable runtime contains a reparse point"
            );
        }
        if metadata.is_dir() {
            let _pin = OpenedDirectory::open(&entry.path())?;
            names(&entry.path(), &relative, found, budget)?;
        } else {
            airk_ipc::private_record::require_regular_unlinked(&entry.path())?;
            found.insert(relative);
        }
    }
    Ok(())
}

fn preflight(
    program: &Path,
    own: &Path,
    allowed: &[&airk_core::install_record::setup_recovery::InstallationImage],
    inventory_path: &Path,
) -> Result<(Vec<u8>, PayloadInventory)> {
    preflight_with(
        program,
        own,
        allowed,
        inventory_path,
        install::platform_signature,
    )
}

fn preflight_with(
    program: &Path,
    own: &Path,
    allowed: &[&airk_core::install_record::setup_recovery::InstallationImage],
    inventory_path: &Path,
    signature: impl Fn(&airk_core::install_record::Signing, &[&Path]) -> Result<(), (String, String)>,
) -> Result<(Vec<u8>, PayloadInventory)> {
    let root = OpenedDirectory::open(program)?;
    airk_ipc::endpoint::require_private_directory_handle(root.as_file())?;
    let original = root.identity()?;
    let metadata = SplitRuntimeMetadata::decode(&bounded(
        &program.join("airk-runtime.json"),
        MAX_RUNTIME_METADATA_BYTES,
    )?)?;
    metadata.validate_target(
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    )?;
    ensure!(
        metadata.i1_protocol == airk_core::service::PROTOCOL_VERSION
            && metadata.i2_contract == crate::contract::I2_CONTRACT,
        "portable interfaces are incompatible"
    );
    let bytes = bounded(inventory_path, MAX_INVENTORY_BYTES)?;
    let inventory = PayloadInventory::decode(&bytes, digest(&bytes))?;
    ensure!(
        inventory.version == metadata.version
            && inventory.executables.airk == metadata.service_relative_path()?
            && inventory.executables.airkctl == metadata.cli_relative_path()?
            && inventory.admin_assets.inventory_sha256 == metadata.admin_inventory_sha256,
        "portable inventory and C7 metadata differ"
    );
    let target = match (metadata.os.as_str(), metadata.arch.as_str()) {
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        _ => anyhow::bail!("unsupported portable target"),
    };
    ensure!(
        inventory.target == target,
        "portable inventory target differs from C7"
    );
    let service = inventory
        .files
        .get(&inventory.executables.airk)
        .context("portable service missing")?;
    let cli = inventory
        .files
        .get(&inventory.executables.airkctl)
        .context("portable CLI missing")?;
    ensure!(
        service.sha256 == metadata.airk_sha256 && cli.sha256 == metadata.airkctl_sha256,
        "portable images differ from C7 metadata"
    );
    ensure!(
        own.canonicalize()?
            == program
                .join(&inventory.executables.airkctl)
                .canonicalize()?,
        "run portable setup using this runtime's own CLI"
    );
    let mut found = BTreeSet::new();
    let mut budget = 8192;
    names(program, "", &mut found, &mut budget)?;
    if found.contains("install-record.json") {
        let bytes = bounded(
            &program.join("install-record.json"),
            airk_core::install_record::MAX_RECORD_BYTES,
        )?;
        ensure!(
            allowed
                .iter()
                .any(|image| canonical(&image.record) == bytes),
            "program record is not owned by this exact current/resuming operation"
        );
        found.remove("install-record.json");
    }
    if found.contains(INVENTORY_FILE) {
        let existing = bounded(&program.join(INVENTORY_FILE), MAX_INVENTORY_BYTES)?;
        if existing != bytes {
            let old = PayloadInventory::decode(&existing, digest(&existing))?;
            ensure!(
                allowed
                    .iter()
                    .any(|image| old.version == image.record.version
                        && old.channel == image.record.channel
                        && old.target == image.record.target
                        && old.admin_assets == image.record.admin_assets
                        && old.signing.as_ref() == Some(&image.record.signing)
                        && old
                            .files
                            .get(&old.executables.airk)
                            .is_some_and(|file| file.sha256
                                == image.record.executables.airk.sha256
                                && file.bytes == image.record.executables.airk.bytes)
                        && old
                            .files
                            .get(&old.executables.airkctl)
                            .is_some_and(|file| file.sha256
                                == image.record.executables.airkctl.sha256
                                && file.bytes == image.record.executables.airkctl.bytes)),
                "program inventory is not owned by prior I4"
            );
        }
        found.remove(INVENTORY_FILE);
    }
    ensure!(
        found == inventory.files.keys().cloned().collect(),
        "portable program contains unapproved files"
    );
    for (name, file) in &inventory.files {
        ensure!(
            file_digest(&program.join(name), file.bytes)? == file.sha256,
            "portable payload differs: {name}"
        );
    }
    signature(
        inventory
            .signing
            .as_ref()
            .context("portable signing identity missing")?,
        &[
            program.join(&inventory.executables.airkctl).as_path(),
            program.join(&inventory.executables.airk).as_path(),
        ],
    )
    .map_err(|(code, message)| anyhow::anyhow!("{code}: {message}"))?;
    ensure!(
        OpenedDirectory::open(program)?.identity()? == original,
        "portable root changed during preflight"
    );
    Ok((bytes, inventory))
}

fn invoke_plan(airk: &Path, operation: &Path) -> Result<()> {
    let operation = operation
        .to_str()
        .context("private operation path is not UTF-8")?;
    let args = ["__airk_role", "setup", "--operation", operation, "--plan"];
    let output = airk_process::bounded_command::execute_with_timeout(
        &airk_process::bounded_command::Invocation::new(airk, &args),
        std::time::Duration::from_millis(MAX_SETUP_OPERATION_LIFETIME_MS),
    )?;
    ensure!(
        output.success,
        "native portable setup failed: {}",
        output.text
    );
    Ok(())
}

pub(crate) fn setup(options: Options) -> Result<SetupReceiptObservation, ErrorLine> {
    let mut recovery = None::<PathBuf>;
    let mut effects_possible = false;
    let mut process_id = None;
    let mut original = None;
    let result = (|| -> Result<SetupReceiptObservation> {
        if let Some(path) = &options.resume {
            recovery = Some(path.clone());
            return resume(path, &mut effects_possible, &mut process_id, &mut original);
        }
        let program = airk_process::user_folders::checked(
            options.program.clone().context("--program is required")?,
        )?;
        let pointer = install::pointer_path()?;
        let existing = airk_ipc::install_reader::read_metadata(&pointer)
            .map_err(|(code, message)| anyhow::anyhow!("{code}: {message}"))?;
        if let Some(current) = &existing {
            ensure!(
                current.record.installed_by.installer
                    == airk_core::install_record::Installer::Portable,
                "an existing nonportable installation requires its own repair/removal flow"
            );
        }
        let same_program = existing.as_ref().filter(|current| {
            Path::new(&current.pointer.program).canonicalize().ok() == program.canonicalize().ok()
        });
        let allowed = same_program.into_iter().collect::<Vec<_>>();
        let inventory_path = program
            .parent()
            .context("portable runtime parent missing")?
            .join(INVENTORY_FILE);
        let (inventory_bytes, inventory) = preflight(
            &program,
            &std::env::current_exe()?,
            &allowed,
            &inventory_path,
        )?;
        let locations = locations(&options, existing.as_ref())?;
        if let Some(current) = &existing {
            ensure!(
                locations.data == current.record.locations.data
                    && locations.state == current.record.locations.state
                    && locations.cache == current.record.locations.cache
                    && locations.logs == current.record.locations.logs,
                "portable repair preserves the existing data, state, cache and logs locations"
            );
        }
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).map_err(|error| anyhow::anyhow!("operation ID: {error}"))?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as u64;
        let request = operation(
            locations,
            Incarnation::from_bytes(nonce),
            digest(&inventory_bytes),
            now,
        );
        original = Some(request.clone());
        SetupOperation::decode(&canonical(&request), now)
            .map_err(|error| anyhow::anyhow!("{}", error.code()))?;
        let pointer = install::pointer_path()?;
        let authority = pointer.parent().context("pointer parent missing")?;
        airk_ipc::endpoint::prepare_root(authority)?;
        let authority_pin = OpenedDirectory::open(authority)?;
        let operations_pin = match authority_pin.open_child(std::ffi::OsStr::new("operations")) {
            Ok(pin) => pin,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                airk_ipc::endpoint::create_root_at(
                    &authority_pin,
                    std::ffi::OsStr::new("operations"),
                )?
            }
            Err(error) => return Err(error.into()),
        };
        let operations = authority.join("operations");
        let folder = operations.join(request.operation_id.to_string());
        let folder_pin = airk_ipc::endpoint::create_root_at(
            &operations_pin,
            std::ffi::OsStr::new(&request.operation_id.to_string()),
        )?;
        let path = folder.join("operation.json");
        recovery = Some(folder.join("operation-request.json"));
        for name in ["operation.json", "operation-request.json"] {
            airk_ipc::private_record::write_private_at(
                &folder_pin,
                name,
                &canonical(&request),
                airk_core::install_record::MAX_SETUP_OPERATION_BYTES,
                false,
            )?;
        }
        airk_ipc::private_record::write_private_at(
            &folder_pin,
            INVENTORY_FILE,
            &inventory_bytes,
            MAX_INVENTORY_BYTES,
            false,
        )?;
        folder_pin.sync_all()?;
        let airk = program.join(&inventory.executables.airk);
        invoke_plan(&airk, &path)?;
        let plan: serde_json::Value =
            serde_json::from_slice(&bounded(&folder.join("plan.json"), 256 * 1024)?)?;
        ensure!(
            plan["schema"] == "airk-setup-plan/1"
                && plan["operation_id"] == request.operation_id.to_string(),
            "native plan is unbound"
        );
        ensure!(
            plan["location_problems"]
                .as_array()
                .is_some_and(Vec::is_empty)
                && plan["blockers"].as_array().is_some_and(Vec::is_empty)
                && plan["leftovers"].as_array().is_some_and(|rows| rows
                    .iter()
                    .all(|row| row["restarts_old_service"] == false))
                && plan["predecessors"].as_array().is_some_and(|rows| rows
                    .iter()
                    .all(|row| row["removed"] == false && row.get("refused").is_none())),
            "portable setup cannot continue; review the retained plan and use the supported installer for predecessor/reset decisions"
        );
        effects_possible = true;
        complete(&airk, &path, &mut process_id)?;
        let current = match install::discover() {
            install::Discovery::Verified(current) => current,
            _ => anyhow::bail!("portable publication is not verified"),
        };
        ensure!(
            current.record.installed_by.operation_id == request.operation_id
                && current.record.locations == request.locations
                && current.record.installed_by.installer
                    == airk_core::install_record::Installer::Portable,
            "the installation changed after native setup"
        );
        let observation = crate::setup_receipt::observe(&current)
            .map_err(|error| anyhow::anyhow!("{}", error.message))?;
        ensure!(
            observation.successful,
            "native portable setup did not close successfully"
        );
        Ok(observation)
    })();
    result.map_err(|error|{
        let mut line=ErrorLine::new(if effects_possible{ExitClass::OutcomeUnknown}else{ExitClass::Refused},
            if effects_possible{"portable_setup_outcome_unknown"}else{"portable_setup_failed"},false,
            &format!("{}{error:#}",recovery.as_ref().map(|path|format!("retained original operation {}; ",path.display())).unwrap_or_default()));
        if effects_possible {
            let observation=original.as_ref().map(original_receipt).transpose();
            line.native=Some(match observation {
                Ok(receipt)=>serde_json::json!({"operation":recovery,"process_id":process_id,"status":"outcome_unknown","receipt":receipt.flatten()}),
                Err(error)=>serde_json::json!({"operation":recovery,"process_id":process_id,"status":"outcome_unknown","receipt_observation_error":format!("{error:#}")}),
            });
        }
        line
    })
}

fn complete(airk: &Path, operation: &Path, pid: &mut Option<u32>) -> Result<()> {
    let mut child = crate::uninstall::recovery_child::launch(airk, operation)
        .map_err(|line| anyhow::anyhow!("{}", line.message))?;
    *pid = Some(child.id());
    ensure!(
        child
            .wait(std::time::Duration::from_millis(
                MAX_SETUP_OPERATION_LIFETIME_MS
            ))
            .map_err(|line| anyhow::anyhow!("{}", line.message))?,
        "native portable setup remains pending; the exact child was preserved, never killed; reconcile the original operation"
    );
    Ok(())
}

fn native_identity(
    directory: &OpenedDirectory,
) -> Result<airk_core::install_record::setup_recovery::RootIdentity> {
    use airk_core::install_record::setup_recovery::RootIdentity;
    use airk_process::directory::ObjectIdentity;
    Ok(match directory.identity()? {
        ObjectIdentity::Unix { device, inode } => RootIdentity::Unix { device, inode },
        ObjectIdentity::Windows {
            volume_serial,
            file_id,
        } => RootIdentity::Windows {
            volume_serial,
            file_id: file_id.iter().map(|byte| format!("{byte:02x}")).collect(),
        },
    })
}

fn original_receipt(
    operation: &SetupOperation,
) -> Result<Option<airk_core::install_record::SetupReceipt>> {
    let folder = match OpenedDirectory::open(&Path::new(&operation.locations.state).join("setup")) {
        Ok(folder) => folder,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let Some(raw) = airk_ipc::private_record::read_private_at(
        &folder,
        &format!("{}.json", operation.operation_id),
        airk_core::install_record::MAX_SETUP_RECEIPT_BYTES,
    )?
    else {
        return Ok(None);
    };
    let receipt = airk_core::install_record::SetupReceipt::decode(&raw)
        .map_err(|error| anyhow::anyhow!("{}", error.code()))?;
    ensure!(
        receipt.operation_id == operation.operation_id
            && receipt.operation_sha256 == digest(&canonical(operation))
            && receipt.kind == operation.kind
            && receipt.expires_unix_ms == operation.expires_unix_ms,
        "original setup receipt binding changed"
    );
    Ok(Some(receipt))
}

fn decode_original(
    raw: &[u8],
    now: u64,
    admission: Option<(Incarnation, Sha256Digest, u64)>,
) -> Result<SetupOperation> {
    let at = admission.map_or(now, |(_, _, at)| at);
    let operation =
        SetupOperation::decode(raw, at).map_err(|error| anyhow::anyhow!("{}", error.code()))?;
    ensure!(
        operation.requested_by == "airkctl-portable",
        "only original portable operations can resume here"
    );
    if let Some((id, expected, _)) = admission {
        ensure!(
            id == operation.operation_id && expected == digest(raw),
            "original operation changed after admission"
        );
    }
    Ok(operation)
}

fn observe_locations(
    locations: &Locations,
    pins: &mut Vec<OpenedDirectory>,
) -> Result<airk_core::install_record::setup_recovery::active::RootSnapshot> {
    use airk_core::install_record::setup_recovery::{
        RetainedRoots, RootObservation, active::RootSnapshot,
    };
    let mut observe = |path: &str| -> Result<RootObservation> {
        match OpenedDirectory::open(Path::new(path)) {
            Ok(pin) => {
                airk_ipc::endpoint::require_private_directory_handle(pin.as_file())?;
                let identity = native_identity(&pin)?;
                pins.push(pin);
                Ok(RootObservation::Present { identity })
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(RootObservation::Absent)
            }
            Err(error) => Err(error.into()),
        }
    };
    Ok(RootSnapshot {
        program: observe(&locations.program)?,
        retained: RetainedRoots {
            data: observe(&locations.data)?,
            state: observe(&locations.state)?,
            cache: observe(&locations.cache)?,
            logs: observe(&locations.logs)?,
        },
    })
}

fn resume(
    path: &Path,
    effects_possible: &mut bool,
    pid: &mut Option<u32>,
    original: &mut Option<SetupOperation>,
) -> Result<SetupReceiptObservation> {
    use airk_core::install_record::setup_recovery::active::{ActiveSetup, Stage, Target};
    ensure!(
        path.is_absolute(),
        "--resume must name an absolute original private operation"
    );
    let folder = OpenedDirectory::open(path.parent().context("operation parent missing")?)?;
    airk_ipc::endpoint::require_private_directory_handle(folder.as_file())?;
    let raw = airk_ipc::private_record::read_private_at(
        &folder,
        path.file_name()
            .and_then(|name| name.to_str())
            .context("operation name is not UTF-8")?,
        airk_core::install_record::MAX_SETUP_OPERATION_BYTES,
    )?
    .context("original private operation is missing")?;
    let preview: SetupOperation = serde_json::from_slice(&raw)?;
    let preview = decode_original(&raw, preview.created_unix_ms, None)?;
    *original = Some(preview.clone());
    let pointer = install::pointer_path()?;
    let authority = OpenedDirectory::open(pointer.parent().context("pointer parent missing")?)?;
    let active = airk_ipc::private_record::read_private_at(
        &authority,
        "active-setup.json",
        airk_core::install_record::setup_recovery::MAX_RECOVERY_BYTES,
    )?
    .map(|bytes| {
        ActiveSetup::decode(&bytes, &preview)
            .map_err(|error| anyhow::anyhow!("changed or competing setup owner: {}", error.code()))
    })
    .transpose()?;
    let mut pins = Vec::new();
    if let Some(owner) = &active {
        *effects_possible = true;
        let selected = observe_locations(&owner.operation.locations, &mut pins)?;
        let prior = match &owner.target {
            Target::Place { publication } => publication
                .prior
                .as_ref()
                .map(|image| observe_locations(&image.record.locations, &mut pins))
                .transpose()?,
            Target::Remove { .. } => None,
        };
        owner
            .validate_live_roots(
                &native_identity(&authority)?,
                &selected,
                prior.as_ref(),
                None,
            )
            .map_err(|error| anyhow::anyhow!("setup roots changed: {}", error.code()))?;
    }
    let mut successful = None;
    let mut current_image = None;
    if let install::Discovery::Verified(current) = install::discover()
        && current.record.installed_by.operation_id == preview.operation_id
        && current.record.locations == preview.locations
        && current.record.installed_by.installer == airk_core::install_record::Installer::Portable
    {
        let observation = crate::setup_receipt::observe(&current)
            .map_err(|line| anyhow::anyhow!("{}", line.message))?;
        if observation.successful {
            successful = Some(observation);
        }
        current_image = Some(
            airk_core::install_record::setup_recovery::InstallationImage {
                pointer: current.pointer.clone(),
                record: current.record.clone(),
            },
        );
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64;
    let operation = if successful.is_some() {
        preview
    } else {
        decode_original(
            &raw,
            now,
            active.as_ref().map(|owner| {
                (
                    owner.operation.operation_id,
                    owner.operation_sha256,
                    owner.admitted_unix_ms,
                )
            }),
        )?
    };
    let program = PathBuf::from(&operation.locations.program);
    let mut allowed = Vec::new();
    if let Some(owner) = &active {
        let Target::Place { publication } = &owner.target else {
            anyhow::bail!("portable resume requires placing operation")
        };
        allowed.push(&publication.candidate);
        if let Some(prior) = &publication.prior
            && prior.pointer.program == operation.locations.program
        {
            allowed.push(prior);
        }
    }
    if let Some(image) = &current_image {
        allowed.push(image);
    }
    let external = path.parent().unwrap().join(INVENTORY_FILE);
    let inventory_path = if external.try_exists()? {
        external
    } else {
        program.join(INVENTORY_FILE)
    };
    let (bytes, inventory) = preflight(
        &program,
        &std::env::current_exe()?,
        &allowed,
        &inventory_path,
    )?;
    ensure!(
        digest(&bytes) == operation.payload_inventory_sha256,
        "original operation inventory changed"
    );
    if let Some(observation) = successful {
        return Ok(observation);
    }
    if let Some(owner) = &active {
        ensure!(
            owner.stage != Stage::Settled,
            "settled operation has no matching successful current receipt"
        );
    } else {
        let receipts = Path::new(&operation.locations.state)
            .join("setup")
            .join(format!("{}.json", operation.operation_id));
        ensure!(
            !receipts.try_exists()?,
            "a consumed or unproven original operation cannot be replayed without its active owner"
        );
    }
    *effects_possible = true;
    complete(&program.join(&inventory.executables.airk), path, pid)?;
    let current = match install::discover() {
        install::Discovery::Verified(current) => current,
        _ => anyhow::bail!("resumed publication is not verified"),
    };
    ensure!(
        current.record.installed_by.operation_id == operation.operation_id
            && current.record.locations == operation.locations,
        "resumed installation changed"
    );
    let observation = crate::setup_receipt::observe(&current)
        .map_err(|line| anyhow::anyhow!("{}", line.message))?;
    ensure!(
        observation.successful,
        "original operation did not close successfully"
    );
    Ok(observation)
}

#[cfg(test)]
mod tests;
