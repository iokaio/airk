// SPDX-License-Identifier: Apache-2.0
//! Read-only capture of incarnation-local settings. This is not a complete
//! installation recovery roster or authority to restore another namespace.
use super::*;

const MAX_SETTINGS_BYTES: u64 = 1 << 20;
const MAX_SETTING_BYTES: u64 = 262144;
const MAX_SLOT_ENTRIES: usize = 128;
type CapturedSettingFile = (&'static str, Option<Vec<u8>>);

/// An observation constructed only while native selection and writer exclusion
/// are retained. Exact bytes and explicit absence are preserved. No deserializer
/// turns a saved observation into native capture authority.
///
/// The catalog covers incarnation-local configuration envelopes only. Registry
/// pause/watch choices, usage policy, source-policy capsules and shared model assets
/// require their separate native readers. Applied Current configuration is
/// retained separately from the saved policy files. Policy payloads
/// remain opaque here; their owning kernel/ingest adapters must interpret them
/// before a recovery may be admitted.
pub struct CapturedCollectionSettings {
    owner_root: String,
    collection: String,
    owner: Sha256Digest,
    incarnation: Incarnation,
    files: Vec<CapturedSettingFile>,
    applied: Option<crate::current::collection::CollectionConfiguration>,
    saved_search_policy: bool,
    digest: Sha256Digest,
}

impl CapturedCollectionSettings {
    pub fn owner_root(&self) -> &str {
        &self.owner_root
    }
    pub fn collection(&self) -> &str {
        &self.collection
    }
    pub fn owner(&self) -> Sha256Digest {
        self.owner
    }
    pub fn incarnation(&self) -> Incarnation {
        self.incarnation
    }
    /// A distinct observation identity; the existing replacement/settings
    /// digest and copy contracts are deliberately unchanged.
    pub fn digest(&self) -> Sha256Digest {
        self.digest
    }
    pub fn files(&self) -> impl ExactSizeIterator<Item = (&'static str, Option<&[u8]>)> {
        self.files
            .iter()
            .map(|(name, bytes)| (*name, bytes.as_deref()))
    }
    pub fn applied_configuration(
        &self,
    ) -> Option<&crate::current::collection::CollectionConfiguration> {
        self.applied.as_ref()
    }
    pub fn has_saved_search_policy(&self) -> bool {
        self.saved_search_policy
    }
}

impl Collection {
    /// Read known saved preference envelopes for a clean rebuild without
    /// opening retained publications or requiring their historical format.
    /// Unknown settings are deliberately outside this reset preference catalog.
    pub fn capture_clean_break_preferences(
        &self,
        include_legacy_roots: bool,
    ) -> Result<CapturedCollectionSettings> {
        let selected = self.select_existing_locked()?;
        let _writer = selected.store.existing_writer_exclusion()?;
        let incarnation = selected
            .store
            .service_incarnation()?
            .context("saved preferences require an owned service incarnation")?;
        let directory =
            crate::store_fs::SetupDirectory::open_canonical(selected.store.directory())?;
        let mut files = Vec::with_capacity(SETTINGS.len());
        let mut total = 0usize;
        for name in SETTINGS {
            if matches!(name, "compression-policy.json" | "scan-limits.json")
                || name == "roots.json" && !include_legacy_roots
            {
                files.push((name, None));
                continue;
            }
            let bytes = match directory.read_file(std::ffi::OsStr::new(name), MAX_SETTING_BYTES) {
                Ok(bytes) => Some(bytes),
                Err(error)
                    if error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
                {
                    None
                }
                Err(error) => return Err(error),
            };
            if let Some(bytes) = &bytes {
                total = total
                    .checked_add(bytes.len())
                    .context("saved preferences size overflow")?;
                ensure!(
                    total <= MAX_SETTINGS_BYTES as usize,
                    "saved preferences exceed byte bound"
                );
                validate_envelope(
                    name,
                    bytes,
                    &self.owner.root,
                    selected.store.collection_id()?,
                )?;
            }
            files.push((name, bytes));
        }
        let digest = framed_digest(
            b"airk/clean-break-preferences/1",
            &[&serde_json::to_vec(&(
                self.binding,
                incarnation,
                &self.owner.root,
                &self.owner.collection,
                &files,
            ))?],
        );
        Ok(CapturedCollectionSettings {
            owner_root: self.owner.root.clone(),
            collection: self.owner.collection.clone(),
            owner: self.binding,
            incarnation,
            files,
            applied: None,
            saved_search_policy: false,
            digest,
        })
    }

    /// Capture only the existing selected incarnation, without creating a
    /// writer marker, policy, publication or source read. Failure to authenticate
    /// any supported file remains a failure, never an absent/default setting.
    pub fn capture_saved_settings(&self) -> Result<CapturedCollectionSettings> {
        let selected = self.select_existing_locked()?;
        validate_split_store_access(&selected.store)?;
        let _writer = selected.store.existing_writer_exclusion()?;
        let incarnation = selected
            .store
            .service_incarnation()?
            .context("saved settings require an owned service incarnation")?;
        let files = capture_files(&selected.store, &self.owner.root)?;
        let (applied, saved_search_policy) = match selected.store.current_owned_scope()? {
            Some(scope) => {
                let read = selected.store.current_read(&scope)?;
                ensure!(
                    !read.has_prepared_update()?,
                    "saved settings publication is pending"
                );
                let applied = read.collection_state()?.map(|state| state.configuration);
                let saved_search_policy =
                    read.resource(&selected.store, "semantic-policy")?.is_some();
                read.token()?;
                (applied, saved_search_policy)
            }
            None => (None, false),
        };
        let digest = framed_digest(
            b"airk/captured-incarnation-settings/1",
            &[&serde_json::to_vec(&(
                self.binding,
                incarnation,
                &self.owner.root,
                &self.owner.collection,
                &files,
                &applied,
                saved_search_policy,
            ))?],
        );
        Ok(CapturedCollectionSettings {
            owner_root: self.owner.root.clone(),
            collection: self.owner.collection.clone(),
            owner: self.binding,
            incarnation,
            files,
            applied,
            saved_search_policy,
            digest,
        })
    }
}

fn capture_files(source: &FileStore, root: &str) -> Result<Vec<CapturedSettingFile>> {
    let directory = crate::store_fs::SetupDirectory::open_canonical(source.directory())?;
    // Unknown future policy files cannot silently disappear from this explicit
    // catalog. Inspect names only; never discover sources from this directory.
    for (index, entry) in directory.entries()?.enumerate() {
        ensure!(
            index < MAX_SLOT_ENTRIES,
            "saved settings directory exceeds bound"
        );
        let name = entry?;
        let name = name.to_str().context("non-UTF-8 saved settings entry")?;
        ensure!(
            !(name.ends_with("-policy.json") || name.ends_with("-limits.json"))
                || SETTINGS.contains(&name),
            "unsupported saved settings file: {name}"
        );
    }
    let mut total = 0u64;
    let mut result = Vec::with_capacity(SETTINGS.len());
    for name in SETTINGS {
        let bytes = match directory.read_file(std::ffi::OsStr::new(name), MAX_SETTING_BYTES) {
            Ok(bytes) => bytes,
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
            {
                result.push((name, None));
                continue;
            }
            Err(error) => return Err(error),
        };
        total = total
            .checked_add(bytes.len() as u64)
            .context("saved settings size overflow")?;
        ensure!(
            bytes.len() as u64 <= MAX_SETTING_BYTES && total <= MAX_SETTINGS_BYTES,
            "saved settings capture exceeds byte bound"
        );
        validate_envelope(name, &bytes, root, source.collection_id()?)?;
        result.push((name, Some(bytes)));
    }
    Ok(result)
}

fn validate_envelope(name: &str, bytes: &[u8], root: &str, collection: &str) -> Result<()> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Policy {
        version: u32,
        owner_root: String,
        #[serde(rename = "policy")]
        _policy: serde_json::Value,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Roots {
        schema: u32,
        owner_root: String,
        roots: std::collections::BTreeMap<String, String>,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Limits {
        schema: u32,
        root: String,
        collection: String,
        limits: serde_json::Value,
    }
    match name {
        "roots.json" => {
            let value: Roots = serde_json::from_slice(bytes)?;
            ensure!(
                value.schema == 1 && value.owner_root == root,
                "foreign saved roots"
            );
            ensure!(value.roots.len() <= 16, "saved roots exceed capacity");
            for (alias, path) in value.roots {
                crate::roots::validate_alias(&alias)?;
                ensure!(
                    path.len() <= 4096 && Path::new(&path).is_absolute(),
                    "invalid saved source root"
                );
            }
        }
        "scan-limits.json" => {
            let value: Limits = serde_json::from_slice(bytes)?;
            ensure!(
                value.schema == 1 && value.root == root && value.collection == collection,
                "foreign saved scan limits"
            );
            ensure!(
                value.limits.is_object(),
                "invalid saved scan limits payload"
            );
        }
        _ => {
            let value: Policy = serde_json::from_slice(bytes)?;
            ensure!(
                value.version == 1 && value.owner_root == root,
                "foreign saved policy"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
