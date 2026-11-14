// SPDX-License-Identifier: Apache-2.0
//! One finalized Inno EXE/DAT custody owner for pending start and outer recheck.
use super::*;
#[cfg(windows)]
use airk_core::install_record::inno_handoff::{
    HANDOFF_FILE, HANDOFF_SCHEMA, InnoHandoff, MAX_HANDOFF_BYTES, ORIGINAL_OPERATION,
    is_uninstaller_name,
};
use airk_core::install_record::setup_recovery::InstallationImage;

pub(super) fn seal(request: &SetupRequest) -> Result<()> {
    #[cfg(windows)]
    {
        let raw = files::read_operation(&request.operation)?;
        let preview: SetupOperation = serde_json::from_slice(&raw)?;
        let pointer = files::pointer_path()?;
        let _lock = files::lock()?;
        let at = transaction::admission_time(&pointer, &preview, files::digest_bytes(&raw))?
            .context("inno_seal_refused: original active owner missing")?;
        let operation = SetupOperation::decode(&raw, at)
            .map_err(|error| anyhow::anyhow!("inno_seal_refused: {}", error.code()))?;
        ensure!(
            operation.requested_by == "inno",
            "inno_seal_refused: wrong installer source"
        );
        let completed = transaction::completed_for(&pointer, &operation)?;
        let airk_core::install_record::setup_recovery::active::Target::Place { publication } =
            completed.target
        else {
            bail!("inno_seal_refused: not placing")
        };
        let installed = files::read_installation()?
            .context("inno_seal_refused: current installation missing")?;
        let image = InstallationImage {
            pointer: installed.pointer,
            record: installed.record,
        };
        ensure!(
            image == publication.candidate,
            "inno_seal_refused: current I4 changed"
        );
        verify_existing(&pointer)?;
        write(&operation, &image)?;
        println!(
            "{}",
            json!({"schema":"airk-inno-handoff-result/1","operation_id":operation.operation_id,"handoff":Path::new(&image.record.locations.program).join(HANDOFF_FILE)})
        );
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = request;
        bail!("inno_seal_unsupported: Inno custody is Windows-only")
    }
}

pub(super) fn seal_published(setup: &mut Setup) -> Result<()> {
    if setup.operation.requested_by != "inno" {
        return Ok(());
    }
    #[cfg(windows)]
    {
        setup.verify_payload()?;
        seal_owner_with(
            setup.owner.as_mut().context("published owner missing")?,
            &setup.operation,
            &files::pointer_path()?,
            write,
        )
    }
    #[cfg(not(windows))]
    {
        bail!("inno_seal_unsupported: Inno source requires Windows")
    }
}

/// Called only after the payload verification gate; the injected writer seam
/// keeps actual root/I4/original-operation reconciliation independently testable.
#[cfg(windows)]
pub(super) fn seal_owner_with(
    owner: &mut transaction::Owner,
    operation: &SetupOperation,
    pointer: &Path,
    writer: impl FnOnce(&SetupOperation, &InstallationImage) -> Result<()>,
) -> Result<()> {
    owner.revalidate()?;
    ensure!(
        owner.active.operation == *operation,
        "inno_seal_refused: original operation differs"
    );
    ensure!(
        owner.active.stage
            == airk_core::install_record::setup_recovery::active::Stage::PairPublished,
        "inno_seal_refused: candidate not published"
    );
    let image = owner.candidate()?.clone();
    let actual = owner.existing(pointer)?.context("published I4 missing")?;
    ensure!(
        actual.pointer == image.pointer && actual.record == image.record,
        "inno_seal_refused: candidate I4 changed"
    );
    writer(operation, &image)
}

pub(super) fn resume_seal_required(pointer_written: bool, action: Action) -> bool {
    pointer_written && matches!(action, Action::RegisterLogin | Action::Start)
}

pub(super) fn require_start_custody(image: &InstallationImage) -> Result<()> {
    require_start_custody_with(image, |signing, paths| {
        airk_process::signature::verify(signature_claim(signing), paths)
            .map(|_| ())
            .map_err(|error| anyhow::anyhow!("{}: {}", error.code, error.message))
    })
}

fn require_start_custody_with(
    image: &InstallationImage,
    signature: impl Fn(&Signing, &[&Path]) -> Result<()>,
) -> Result<()> {
    if image.record.installed_by.installer != Installer::Inno {
        return Ok(());
    }
    #[cfg(windows)]
    {
        let root = airk_process::directory::OpenedDirectory::open(Path::new(
            &image.record.locations.program,
        ))?;
        let raw =
            airk_ipc::private_record::read_private_at(&root, HANDOFF_FILE, MAX_HANDOFF_BYTES)?
                .context("installation_in_progress: finalized Inno custody is not sealed")?;
        let handoff = InnoHandoff::decode(&raw, image)
            .map_err(|error| anyhow::anyhow!("inno_custody_unverified: {}", error.code()))?;
        handoff
            .validate_for(image)
            .map_err(|error| anyhow::anyhow!("inno_custody_unverified: {}", error.code()))?;
        for image_file in [&handoff.uninstaller, &handoff.data] {
            let held = root.open_file(std::ffi::OsStr::new(&image_file.path))?;
            airk_ipc::endpoint::require_private_metadata_handle(&held)?;
            root.require_default_stream(std::ffi::OsStr::new(&image_file.path), &held)?;
            let actual = files::digest_file(
                &Path::new(&image.record.locations.program).join(&image_file.path),
            )?;
            ensure!(
                actual == (image_file.sha256, image_file.bytes),
                "inno_custody_unverified: finalized uninstaller files changed"
            );
        }
        signature(
            &image.record.signing,
            &[Path::new(&image.record.locations.program)
                .join(&handoff.uninstaller.path)
                .as_path()],
        )?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = signature;
        bail!("inno_custody_unverified: Windows-only installer identity")
    }
}

#[cfg(windows)]
fn write(operation: &SetupOperation, image: &InstallationImage) -> Result<()> {
    ensure!(
        image.record.installed_by.installer == Installer::Inno
            && image.record.installed_by.operation_id == operation.operation_id,
        "inno_seal_refused: original candidate identity differs"
    );
    let own = own_image()?;
    ensure!(
        own.1 == image.record.executables.airk.sha256 && image.record.version == VERSION,
        "inno_seal_refused: exact original native image required"
    );
    airk_process::signature::verify(signature_claim(&image.record.signing), &[own.0.as_path()])
        .map_err(|error| anyhow::anyhow!("{}: {}", error.code, error.message))?;
    let program = Path::new(&image.record.locations.program);
    airk_host::runtime_pair::verify_installed_pair(
        &program.join(&image.record.executables.airk.path),
        VERSION,
    )?;
    let root = airk_process::directory::OpenedDirectory::open(program)?;
    airk_ipc::endpoint::require_private_directory_handle(root.as_file())?;
    let mut candidates = Vec::new();
    for (index, entry) in std::fs::read_dir(program)?.enumerate() {
        ensure!(
            index < 8192,
            "inno_seal_refused: uninstaller inventory exceeds bound"
        );
        let entry = entry?;
        if let Some(name) = entry
            .file_name()
            .to_str()
            .filter(|name| is_uninstaller_name(name))
        {
            candidates.push(name.to_owned())
        }
    }
    ensure!(
        candidates.len() == 1,
        "inno_seal_refused: exactly one unins###.exe required"
    );
    let name = &candidates[0];
    let data = format!("{}.dat", name.strip_suffix(".exe").unwrap());
    let exe = root.open_file(std::ffi::OsStr::new(name))?;
    let dat = root.open_file(std::ffi::OsStr::new(&data))?;
    for (name, file) in [(name.as_str(), &exe), (data.as_str(), &dat)] {
        airk_ipc::endpoint::require_private_metadata_handle(file)?;
        root.require_default_stream(std::ffi::OsStr::new(name), file)?;
    }
    let observe = |name: &str| -> Result<Executable> {
        let (sha256, bytes) = files::digest_file(&program.join(name))?;
        Ok(Executable {
            path: name.into(),
            sha256,
            bytes,
        })
    };
    let handoff = InnoHandoff {
        schema: HANDOFF_SCHEMA.into(),
        install_id: image.record.install_id,
        operation_id: operation.operation_id,
        version: image.record.version.clone(),
        capability: ORIGINAL_OPERATION.into(),
        uninstaller: observe(name)?,
        data: observe(&data)?,
    };
    handoff
        .validate_for(image)
        .map_err(|error| anyhow::anyhow!("inno_seal_refused: {}", error.code()))?;
    airk_process::signature::verify(
        signature_claim(&image.record.signing),
        &[program.join(name).as_path()],
    )
    .map_err(|error| anyhow::anyhow!("{}: {}", error.code, error.message))?;
    airk_ipc::private_record::write_owner_at(
        &root,
        HANDOFF_FILE,
        &canonical(&handoff),
        MAX_HANDOFF_BYTES,
        true,
    )?;
    Ok(())
}

#[cfg(all(test, windows))]
mod tests;
