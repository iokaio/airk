// SPDX-License-Identifier: Apache-2.0
//! Checkpoint an exact unpublished native adoption capsule without inventing a
//! Current gate or treating missing ready-publication files as an empty index.
use super::*;
use std::ffi::OsStr;

pub(crate) fn uninitialized_legacy_adoption(
    registry: &ServiceRegistry,
    registration: &ProjectRegistration,
) -> Result<bool> {
    let mut statement = registry.connection.prepare(
        "SELECT substr(payload,1,16385) FROM index_operation_receipts WHERE kind=?1 AND stage='prepared' AND json_extract(payload,'$.request.registration.binding')=?2 LIMIT 2",
    )?;
    let rows = statement
        .query_map(params![KIND, registration.binding.to_string()], |row| {
            row.get::<_, Vec<u8>>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        rows.len() <= 1,
        "checkpoint adoption has ambiguous native preparation"
    );
    let Some(bytes) = rows.first() else {
        return Ok(false);
    };
    ensure!(
        bytes.len() <= MAX_RECEIPT_BYTES,
        "checkpoint adoption preparation exceeds bound"
    );
    let receipt: IndexSetupReceipt = serde_json::from_slice(bytes)?;
    receipt.validate()?;
    if receipt.legacy_adoption.is_none() {
        return Ok(false);
    }
    ensure!(
        receipt.request.registration == *registration
            && receipt.stage == SetupStage::Prepared
            && registry
                .index_setup_receipt(receipt.request.caller, &receipt.request.key)?
                .as_ref()
                == Some(&receipt),
        "checkpoint adoption preparation changed"
    );
    reserved_sources(&registry.connection, &receipt)?;
    let evidence = receipt
        .preparation
        .as_ref()
        .context("checkpoint adoption preparation missing")?;
    ensure!(
        evidence.legacy_adoption == receipt.legacy_adoption,
        "checkpoint adoption intent changed"
    );
    let control = registry.root.canonicalize()?;
    let metadata = metadata::Metadata::format_request(&control, &receipt.request, true)?;
    let expected_storage = framed_digest(
        b"airk/catalog-native-setup-storage/1",
        &[
            control
                .to_str()
                .context("non-UTF-8 checkpoint control")?
                .as_bytes(),
            receipt.request.key.namespace.as_bytes(),
            registration.binding.as_bytes(),
            receipt.operation_id.as_bytes(),
        ],
    );
    ensure!(
        evidence.storage_identity == expected_storage
            && evidence.source_configuration_digest == metadata.configuration,
        "checkpoint adoption lacks exact native storage preparation"
    );
    let data = Path::new(&registration.data_root);
    let root = storage::SetupDirectory::open_canonical(data)?;
    let owner = root.read_file(
        OsStr::new(crate::service::data_scope::OWNER),
        metadata.owner.len() as u64,
    )?;
    ensure!(
        owner == metadata.owner,
        "checkpoint adoption data owner changed"
    );
    crate::service::data_scope::bound_from_existing_setup(
        data,
        &control,
        receipt.request.key.namespace,
        &owner,
    )?
    .check()?;
    let collection = root
        .open_dir(OsStr::new("collections-1.4"))?
        .open_dir(OsStr::new(&registration.collection))?;
    let slot = collection
        .open_dir(OsStr::new("incarnations"))?
        .open_dir(OsStr::new(&metadata.slot))?;
    // Once a publication has begun, use the existing strict Current/legacy
    // observer. Never hide an incomplete writer or a damaged ready index.
    for name in [
        "current.sqlite3",
        "current-owner.json",
        "current-bootstrap.json",
        "index.sqlite3",
    ] {
        if data
            .join("collections-1.4")
            .join(&registration.collection)
            .join("incarnations")
            .join(&metadata.slot)
            .join(name)
            .try_exists()?
        {
            return Ok(false);
        }
    }
    storage::validate_collection(&collection, &metadata, true)?;
    ensure!(
        slot.read_file(OsStr::new("roots.json"), metadata.roots.len() as u64)? == metadata.roots,
        "checkpoint adoption saved roots changed"
    );
    Ok(true)
}
