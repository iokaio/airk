// SPDX-License-Identifier: Apache-2.0
//! Exact existing private history/receipt proof; no current-image authority.
use airk_core::{
    identity::Sha256Digest,
    install_record::{
        MAX_SETUP_RECEIPT_BYTES, SetupReceipt,
        setup_recovery::{
            InstallationImage, MAX_RECOVERY_BYTES, RootIdentity,
            active::{ActiveSetup, Stage, Target},
        },
    },
};
use airk_process::directory::{ObjectIdentity, OpenedDirectory};
use anyhow::{Context, Result, ensure};
use std::{ffi::OsStr, path::Path};

pub fn completed_inventory(
    pointer: &Path,
    image: &InstallationImage,
) -> Result<Option<Sha256Digest>> {
    image
        .validate()
        .map_err(|error| anyhow::anyhow!("invalid I4 image: {}", error.code()))?;
    let authority = OpenedDirectory::open(
        pointer
            .parent()
            .context("install authority parent missing")?,
    )?;
    crate::endpoint::require_private_directory_handle(authority.as_file())?;
    let name = format!("{}.json", image.record.installed_by.operation_id);
    let mut candidates = Vec::new();
    if let Some(raw) =
        crate::private_record::read_private_at(&authority, "active-setup.json", MAX_RECOVERY_BYTES)?
    {
        let preview: ActiveSetup = serde_json::from_slice(&raw)?;
        let active = ActiveSetup::decode(&raw, &preview.operation)
            .map_err(|error| anyhow::anyhow!("private setup owner invalid: {}", error.code()))?;
        if active.operation.operation_id == image.record.installed_by.operation_id {
            ensure!(
                active.stage == Stage::Settled,
                "installation_in_progress: original setup has not settled"
            );
            candidates.push(active);
        }
    }
    match authority.open_child(OsStr::new("setup-history")) {
        Ok(history) => {
            if let Some(raw) =
                crate::private_record::read_private_at(&history, &name, MAX_RECOVERY_BYTES)?
            {
                let preview: ActiveSetup = serde_json::from_slice(&raw)?;
                let active = ActiveSetup::decode(&raw, &preview.operation).map_err(|error| {
                    anyhow::anyhow!("private history invalid: {}", error.code())
                })?;
                ensure!(
                    active.operation.operation_id == image.record.installed_by.operation_id,
                    "private history filename/operation mismatch"
                );
                candidates.push(active);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let identity = match authority.identity()? {
        ObjectIdentity::Unix { device, inode } => RootIdentity::Unix { device, inode },
        ObjectIdentity::Windows {
            volume_serial,
            file_id,
        } => RootIdentity::Windows {
            volume_serial,
            file_id: file_id.iter().map(|byte| format!("{byte:02x}")).collect(),
        },
    };
    let mut approved = None;
    for active in candidates {
        ensure!(
            active.stage == Stage::Settled && active.authority_root == identity,
            "completed private history authority changed"
        );
        let Target::Place { publication } = &active.target else {
            anyhow::bail!("completed inventory history is not placing")
        };
        ensure!(
            publication.candidate == *image,
            "completed private history names another I4 image"
        );
        let setup =
            OpenedDirectory::open(&Path::new(&active.operation.locations.state).join("setup"))?;
        let raw = crate::private_record::read_private_at(
            &setup,
            &format!("{}.json", active.operation.operation_id),
            MAX_SETUP_RECEIPT_BYTES,
        )?
        .context("completed private history receipt missing")?;
        let receipt = SetupReceipt::decode(&raw)
            .map_err(|error| anyhow::anyhow!("original receipt invalid: {}", error.code()))?;
        active
            .validate_settlement(&receipt)
            .map_err(|error| anyhow::anyhow!("original setup incomplete: {}", error.code()))?;
        let digest = active.operation.payload_inventory_sha256;
        ensure!(
            approved.is_none_or(|prior| prior == digest),
            "conflicting original inventory proofs"
        );
        approved = Some(digest);
    }
    Ok(approved)
}

#[cfg(test)]
mod tests;
