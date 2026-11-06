// SPDX-License-Identifier: Apache-2.0
//! Native initial metadata encodings for descriptor-anchored setup. These use
//! the owning collection's actual private wire types, never frontend JSON.
use super::*;

pub(super) fn initial_owner(data_root: &Path, root: &str, collection: &str) -> Result<Owner> {
    crate::validate_collection(collection)?;
    ensure!(
        data_root.is_absolute(),
        "initial collection storage must be absolute"
    );
    Ok(Owner {
        version: 140,
        root: root.into(),
        data_root: data_root.to_str().context("non-UTF-8 data root")?.into(),
        collection: collection.into(),
        identity_data_root: None,
        required_source_policy: None,
        required_split: None,
    })
}

pub(crate) fn initial_metadata(
    data_root: &Path,
    root: &str,
    collection: &str,
    slot: Incarnation,
) -> Result<Vec<(&'static str, Vec<u8>)>> {
    let owner = initial_owner(data_root, root, collection)?;
    let binding = owner_digest(&owner)?;
    let manifest = Manifest {
        version: 140,
        owner: binding,
        epoch: 1,
        active: slot,
        retired: vec![],
        last_rebuild: None,
    };
    let files = vec![
        ("service-owner.json", serde_json::to_vec(&owner)?),
        ("active.json", serde_json::to_vec(&manifest)?),
        ("manifest.lock", vec![]),
    ];
    ensure!(
        files.iter().all(|(_, bytes)| bytes.len() <= MAX_METADATA),
        "initial native collection metadata exceeds its format budget"
    );
    Ok(files)
}
