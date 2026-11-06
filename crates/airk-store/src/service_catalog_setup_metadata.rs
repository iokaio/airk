// SPDX-License-Identifier: Apache-2.0
//! Read-only native format preflight, shared by draft review and preparation.
#[cfg(any(unix, windows))]
use super::IndexSetupReceipt;
use super::IndexSetupRequest;
use airk_core::{
    catalog_management::catalog_operation_id,
    identity::{Incarnation, Sha256Digest, framed_digest},
};
use anyhow::{Context, Result};
use std::{collections::BTreeMap, path::Path};

pub(crate) struct Metadata {
    pub(crate) owner: Vec<u8>,
    pub(crate) files: Vec<(&'static str, Vec<u8>)>,
    pub(crate) roots: Vec<u8>,
    pub(crate) slot: String,
    pub(crate) configuration: Sha256Digest,
    pub(crate) split_capsule: Option<(String, Vec<u8>)>,
}
impl Metadata {
    #[cfg(any(unix, windows))]
    pub(crate) fn new(control: &Path, receipt: &IndexSetupReceipt) -> Result<Self> {
        receipt.validate()?;
        Self::from_request(control, &receipt.request)
    }
    pub(crate) fn from_request(control: &Path, request: &IndexSetupRequest) -> Result<Self> {
        request.validate()?;
        let aliases = request
            .sources
            .iter()
            .flat_map(|source| {
                source
                    .aliases
                    .iter()
                    .map(|alias| (alias.clone(), source.canonical_path.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let source = Path::new(&request.registration.root);
        let data = Path::new(&request.registration.data_root);
        crate::roots::validate_planned_sources(source, data, &aliases)?;
        let slot: Incarnation = catalog_operation_id(request.caller, &request.key)?;
        let owner =
            crate::service::data_scope::initial_owner_bytes(data, control, request.key.namespace)?;
        let files = crate::service::collection::initial_setup_metadata(
            data,
            &request.registration.root,
            &request.registration.collection,
            slot,
        )?;
        let roots = crate::roots::RootConfig::initial_setup_bytes(source, data, &aliases)?;
        let configuration = framed_digest(
            b"airk/catalog-source-configuration/1",
            &[
                request.registration.root.as_bytes(),
                &serde_json::to_vec(&aliases)?,
            ],
        );
        let metadata = Self {
            owner,
            files,
            roots,
            slot: slot.to_string(),
            configuration,
            split_capsule: None,
        };
        metadata.encoded_bytes()?;
        Ok(metadata)
    }
    pub(crate) fn new_split(
        control: &Path,
        receipt: &IndexSetupReceipt,
        state: &crate::service::registry::catalog::split::NativeSplitState,
    ) -> Result<Self> {
        let mut metadata = Self::new(control, receipt)?;
        let request = &state.snapshot().request;
        anyhow::ensure!(
            receipt.operation_id == request.operation_id()?
                && receipt.request.registration == request.destination_registration,
            "split setup format context differs from native intent"
        );
        metadata.split_capsule = Some(crate::service::collection::initial_split_seal(
            &mut metadata.files,
            state,
        )?);
        metadata.encoded_bytes()?;
        Ok(metadata)
    }
    pub(crate) fn collection_files(&self) -> Vec<(&str, &[u8])> {
        self.files
            .iter()
            .map(|(name, bytes)| (*name, bytes.as_slice()))
            .chain(
                self.split_capsule
                    .iter()
                    .map(|(name, bytes)| (name.as_str(), bytes.as_slice())),
            )
            .collect()
    }
    pub(crate) fn digest(&self, request: &IndexSetupRequest) -> Result<Sha256Digest> {
        Ok(framed_digest(
            b"airk/catalog-native-setup-encodings/1",
            &[&serde_json::to_vec(&(
                request.identity()?,
                &self.owner,
                &self.files,
                &self.roots,
                &self.slot,
                self.configuration,
                self.encoded_bytes()?,
            ))?],
        ))
    }
    pub(crate) fn collection_bytes(&self) -> Result<u64> {
        self.collection_files()
            .into_iter()
            .map(|(_, bytes)| bytes.len() as u64)
            .try_fold(self.roots.len() as u64, |total, bytes| {
                total
                    .checked_add(bytes)
                    .context("setup metadata byte budget overflow")
            })
    }
    pub(crate) fn encoded_bytes(&self) -> Result<u64> {
        self.collection_bytes()?
            .checked_add(self.owner.len() as u64)
            .context("setup metadata byte budget overflow")
    }
}
