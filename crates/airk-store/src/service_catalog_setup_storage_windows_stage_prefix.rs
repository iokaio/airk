// SPDX-License-Identifier: Apache-2.0
//! Read-only recognition of the same saved setup's private unpublished stage.
//! This includes native collection prefixes written by the earlier staging order.
use super::{
    Metadata, SetupDirectory, SetupFileMode, data_scope, file_identity, validate_collection,
};
use anyhow::{Result, ensure};
use std::{ffi::OsStr, fs::File};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CollectionPrefix {
    Absent,
    Present { published_complete: bool },
}
fn writers(directory: &SetupDirectory, collection: &str) -> Result<()> {
    for (number, name) in directory.entries()?.enumerate() {
        ensure!(number == 0, "unknown private-stage writer preserved");
        let name = name?;
        ensure!(name == collection, "foreign private-stage writer preserved");
        ensure!(
            directory.read_file(&name, 0)?.is_empty(),
            "unknown private-stage writer contents"
        );
    }
    Ok(())
}
/// The known private stage never admits other collections, published index data,
/// other operation slots or arbitrary records. Every permitted byte comes from
/// the owning request's actual native encoders/validators.
pub(super) fn validate_known_private_stage(
    root: &SetupDirectory,
    metadata: &Metadata,
    claim: &File,
    stage_name: &OsStr,
    collection: &str,
) -> Result<CollectionPrefix> {
    let mut owner = false;
    let mut lock = false;
    let mut namespace = false;
    let mut staged = false;
    let mut published = false;
    let mut published_complete = false;
    for (number, name) in root.entries()?.enumerate() {
        ensure!(number < 6, "unknown private bound-root entry preserved");
        let name = name?;
        if name == data_scope::OWNER {
            ensure!(
                root.read_file(&name, metadata.owner.len() as u64)? == metadata.owner,
                "private bound-root owner changed"
            );
            owner = true;
        } else if name == data_scope::LOCK {
            let actual = root.open_file(&name, SetupFileMode::Read)?;
            ensure!(
                actual.metadata()?.len() == 0 && file_identity(&actual)? == file_identity(claim)?,
                "private bound-root claim identity changed"
            );
            lock = true;
        } else if crate::quota::initial_service_claim_entry(&name) {
            // The owning quota validator inspects these formats and allocations.
        } else if name == stage_name {
            validate_collection(&root.open_dir(&name)?, metadata, false)?;
            staged = true;
        } else if name == "collections-1.4" {
            namespace = true;
            let directory = root.open_dir(&name)?;
            for (number, name) in directory.entries()?.enumerate() {
                ensure!(
                    number < 2,
                    "unknown private collection namespace entry preserved"
                );
                let name = name?;
                if name == ".writer-locks" {
                    writers(&directory.open_dir(&name)?, collection)?;
                } else {
                    ensure!(name == collection, "foreign private collection preserved");
                    let target = directory.open_dir(&name)?;
                    validate_collection(&target, metadata, false)?;
                    published_complete = validate_collection(&target, metadata, true).is_ok();
                    published = true;
                }
            }
        } else {
            anyhow::bail!("unknown private bound-root entry preserved");
        }
    }
    ensure!(owner && lock, "private bound-root ownership is incomplete");
    ensure!(
        !(published && staged),
        "duplicate private collection stages preserved"
    );
    if namespace || staged {
        Ok(CollectionPrefix::Present { published_complete })
    } else {
        Ok(CollectionPrefix::Absent)
    }
}
