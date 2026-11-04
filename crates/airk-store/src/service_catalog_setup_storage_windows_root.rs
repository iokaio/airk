// SPDX-License-Identifier: Apache-2.0
//! Missing-root Windows publication before collection preparation retains child
//! handles. NTFS on the probed host refuses ancestor rename with descendants
//! open. Bind OWNER first; recognize older native prefixes without discarding them.
use super::{
    IndexSetupReceipt, Metadata, SetupDirectory, allocation, check_named_pin, data_scope,
    open_or_create,
};
use anyhow::{Context, Result, ensure};
use std::{ffi::OsStr, fs::File, path::Path};

/// Caller retains actual parent/root/source objects and the exact ProjectLease.
/// Before publication the canonical OWNER labels the final path, so other normal
/// native operations cannot bind this private stage as their logical data root.
pub(super) fn publish_owner_bound_root(
    parent: &SetupDirectory,
    root: &SetupDirectory,
    control: &Path,
    stage_name: &OsStr,
    metadata: &Metadata,
    receipt: &IndexSetupReceipt,
    claim: File,
) -> Result<File> {
    use super::windows_stage_prefix::{CollectionPrefix, validate_known_private_stage};
    use crate::quota::setup::{
        InitialCollectionClaim, InitialCollectionClaimState, settle_initial_stage_claims,
    };
    let data_path = Path::new(&receipt.request.registration.data_root);
    let collection = &receipt.request.registration.collection;
    let prefix = validate_known_private_stage(root, metadata, &claim, stage_name, collection)?;
    let collection_claim = match prefix {
        CollectionPrefix::Absent => None,
        CollectionPrefix::Present { published_complete } => Some(InitialCollectionClaim {
            allocation: allocation(receipt, b"collection")?,
            collection,
            bytes: metadata.collection_bytes()?,
            state: if published_complete {
                InitialCollectionClaimState::CompletePublishedMetadata
            } else {
                InitialCollectionClaimState::PreserveAdmission
            },
        }),
    };
    settle_initial_stage_claims(
        root,
        allocation(receipt, b"owner")?,
        metadata.owner.len() as u64,
        collection_claim,
    )?;
    ensure!(
        validate_known_private_stage(root, metadata, &claim, stage_name, collection)? == prefix,
        "private stage changed during initial-root validation"
    );
    check_named_pin(parent, data_path.parent().context("setup parent missing")?)?;
    root.sync()?;
    // No live collection/slot/selection/writer capability or Prepared proof is
    // carried across this boundary. Legacy-prefix validation and quota helpers
    // have closed all descendants; partial admission remains on disk unchanged.
    // Close the claim while continuous root-HANDLE authority survives the gap.
    drop(claim);
    parent.publish_no_replace(
        stage_name,
        &root.file,
        parent,
        data_path.file_name().context("setup leaf missing")?,
    )?;
    #[cfg(test)]
    if std::env::var_os("AIRK_TEST_SETUP_BOUND_ROOT_PUBLISH_CRASH").is_some() {
        std::process::exit(77);
    }
    let claim = open_or_create(root, OsStr::new(data_scope::LOCK))?;
    ensure!(
        claim.metadata()?.len() == 0,
        "unknown published-root claim contents"
    );
    claim
        .try_lock()
        .context("native published data ownership claim is busy")?;
    check_named_pin(root, data_path)?;
    let actual = root.read_file(OsStr::new(data_scope::OWNER), metadata.owner.len() as u64)?;
    ensure!(actual == metadata.owner, "published data ownership changed");
    data_scope::bound_from_existing_setup(
        data_path,
        control,
        receipt.request.key.namespace,
        &actual,
    )?
    .check()?;
    Ok(claim)
}
