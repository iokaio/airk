// SPDX-License-Identifier: Apache-2.0
//! Private bounded policy capture. No source document/model I/O or wire authority.
use super::*;
use crate::store_fs::SetupDirectory;
use airk_core::catalog_split::SplitRequest;
use airk_core::retention::SplitExtractionPolicyReference;
use std::ffi::OsStr;

pub struct CapturedSplitExtractionPolicy {
    request: Sha256Digest,
    body: Vec<u8>,
}
pub struct SavedSplitExtractionPolicy {
    request: Sha256Digest,
    body: Vec<u8>,
}
impl CapturedSplitExtractionPolicy {
    pub(crate) fn validate_for(&self, request: &SplitRequest) -> Result<&[u8]> {
        ensure!(
            self.request == request.identity()?,
            "captured split policy crossed request"
        );
        request
            .extraction_policy
            .as_ref()
            .context("split policy reference absent")?
            .validate_body(&self.body)?;
        Ok(&self.body)
    }
}
impl SavedSplitExtractionPolicy {
    pub(in crate::service) fn from_record(request: &SplitRequest, body: Vec<u8>) -> Result<Self> {
        request
            .extraction_policy
            .as_ref()
            .context("saved split policy reference absent")?
            .validate_body(&body)?;
        Ok(Self {
            request: request.identity()?,
            body,
        })
    }
    pub fn bytes_for(&self, request: &SplitRequest) -> Result<&[u8]> {
        ensure!(
            self.request == request.identity()?,
            "saved split policy crossed request"
        );
        request
            .extraction_policy
            .as_ref()
            .context("saved split policy reference absent")?
            .validate_body(&self.body)?;
        Ok(&self.body)
    }
}
impl Collection {
    /// Read fixed private policy under existing writer/manifest exclusion. The native
    /// draft has already parsed semantics; exact body/ref/settings/source still bind here.
    pub fn capture_split_extraction_policy(
        &self,
        store: &FileStore,
        request: &SplitRequest,
    ) -> Result<CapturedSplitExtractionPolicy> {
        request.validate()?;
        let reference: &SplitExtractionPolicyReference = request
            .extraction_policy
            .as_ref()
            .context("no reviewed extraction policy")?;
        let _writer = store.existing_writer_exclusion()?;
        let _manifest = manifest_lock(&self.directory)?;
        let manifest = self.manifest()?;
        let logical = SetupDirectory::open_canonical(&self.directory)?;
        let owner: Owner = super::retention_barrier::read_json(
            &logical,
            "service-owner.json",
            MAX_METADATA as u64,
        )?;
        let actual = airk_core::service::ProjectRegistration::new(
            owner.root.clone(),
            owner.data_root.clone(),
            owner.collection.clone(),
        )?;
        let selected = logical
            .open_dir(OsStr::new("incarnations"))?
            .open_dir(OsStr::new(&manifest.active.to_string()))?;
        ensure!(
            owner.version == 140
                && owner_digest(&owner)? == self.binding
                && actual == request.original_registration
                && SetupDirectory::open_canonical(store.directory())?.identity()?
                    == selected.identity()?
                && store.service_incarnation()? == Some(request.original_incarnation),
            "split policy capture differs from actual original owner"
        );
        let scope = store
            .current_scope(&self.owner.root, &self.owner.collection)?
            .context("split retained profile needs current publication")?;
        let read = store.current_read(&scope)?;
        let applied = read
            .collection_state()?
            .context("split applied configuration missing")?
            .configuration;
        ensure!(
            applied.retained_profile == Some(reference.retained_profile)
                && applied.index_detail == reference.legacy_fallback_detail
                && applied.extraction_policy_digest == reference.configured_policy_digest,
            "split retained profile is not exactly applied"
        );
        drop(read);
        ensure!(
            settings_id(&settings(store, &self.owner.root)?) == request.original_settings_digest,
            "split extraction policy settings changed"
        );
        let body = crate::extraction_policy::extraction_policy_bytes_at(&selected, &owner.root)?
            .context("reviewed original extraction policy disappeared")?;
        reference.validate_body(&body)?;
        ensure!(
            SetupDirectory::open_canonical(&self.directory)?.identity()? == logical.identity()?
                && SetupDirectory::open_canonical(store.directory())?.identity()?
                    == selected.identity()?,
            "split policy owner directory changed during capture"
        );
        Ok(CapturedSplitExtractionPolicy {
            request: request.identity()?,
            body,
        })
    }
}
