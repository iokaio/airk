// SPDX-License-Identifier: Apache-2.0
//! Conservative saved-definition adapter for an explicitly selected fresh rebuild.
//! The installer supplies process/manager exclusion before admission. These
//! serializable values are retained observations, never filesystem authority.
use airk_core::{
    extraction::IndexDetail,
    identity::{Incarnation, Sha256Digest},
    model_objects::ModelObjectSet,
    service::ProjectRegistration,
};
use airk_store::service::{collection::Collection, registry::ServiceRegistry};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

const MAX_INDEXES: usize = 1024;
const MAX_ROSTER_BYTES: usize = 8 << 20;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SavedRoster {
    pub namespace: Incarnation,
    pub revision: u64,
    pub indexes: Vec<SavedIndex>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SavedIndex {
    pub registration: ProjectRegistration,
    pub paused: bool,
    pub watch: bool,
    pub roots: BTreeMap<String, String>,
    pub vectors_enabled: bool,
    pub index_detail: IndexDetail,
    pub incarnation: Incarnation,
    pub settings_digest: Sha256Digest,
    pub default_model: Option<ModelObjectSet>,
    /// Exact original envelope bytes and explicit absence, retained as evidence.
    /// The new native owner reapplies only the validated supported choices.
    pub saved_files: BTreeMap<String, Option<Vec<u8>>>,
}

impl SavedRoster {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.indexes.len() <= MAX_INDEXES,
            "rebuild index roster exceeds bound"
        );
        let mut bindings = BTreeSet::new();
        let mut previous = None;
        for index in &self.indexes {
            index.validate()?;
            ensure!(
                bindings.insert(index.registration.binding)
                    && previous.is_none_or(|old| old < index.registration.binding),
                "duplicate or unordered rebuild registration"
            );
            previous = Some(index.registration.binding);
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_ROSTER_BYTES,
            "rebuild roster byte bound exceeded"
        );
        Ok(())
    }
    pub fn check(&self, control: &Path) -> Result<()> {
        self.check_observation(&capture(control)?)
    }
    fn check_observation(&self, current: &Self) -> Result<()> {
        self.validate()?;
        current.validate()?;
        // The original registry revision remains part of the admitted receipt
        // identity. Ordinary drained source/watch work may advance this general
        // metadata clock without changing a saved rebuild choice. Compare the
        // complete roster byte-for-byte, not only a projected set of settings;
        // this exception grants no evidence/publication continuity.
        ensure!(
            current.namespace == self.namespace
                && current.revision >= self.revision
                && serde_json::to_vec(&current.indexes)? == serde_json::to_vec(&self.indexes)?,
            "saved rebuild definitions changed"
        );
        Ok(())
    }
}

impl SavedIndex {
    pub fn validate(&self) -> Result<()> {
        self.registration.validate()?;
        ensure!(
            self.roots.len() <= 16,
            "rebuild additional root bound exceeded"
        );
        let mut paths = BTreeSet::new();
        for (alias, path) in &self.roots {
            airk_store::roots::validate_alias(alias)?;
            ensure!(
                Path::new(path).is_absolute()
                    && path.len() <= 4096
                    && path != &self.registration.root
                    && paths.insert(path),
                "invalid rebuild additional root"
            );
        }
        ensure!(
            self.saved_files
                .values()
                .filter_map(Option::as_ref)
                .all(|bytes| bytes.len() <= 262144)
                && self
                    .saved_files
                    .keys()
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>()
                    == BTreeSet::from([
                        "roots.json",
                        "extraction-policy.json",
                        "vector-policy.json",
                        "compression-policy.json",
                        "scan-limits.json"
                    ]),
            "saved rebuild file catalog changed"
        );
        ensure!(
            self.saved_files
                .values()
                .filter_map(Option::as_ref)
                .map(Vec::len)
                .sum::<usize>()
                <= 1 << 20,
            "saved rebuild settings exceed bound"
        );
        let choices = supported_choices(
            &self.saved_files,
            Some(self.index_detail),
            None,
            &self.registration.root,
        )?;
        ensure!(
            choices.roots == self.roots
                && choices.vectors_enabled == self.vectors_enabled
                && choices.index_detail == self.index_detail
                && choices.default_model == self.default_model,
            "saved rebuild choices disagree with original settings"
        );
        Ok(())
    }
}

/// The caller retains the native stop/quiescence proof through capture,
/// revalidation and durable admission. No source directories are read here.
pub(super) fn capture(control: &Path) -> Result<SavedRoster> {
    let control = control.canonicalize()?;
    let mut registry = ServiceRegistry::open_saved(&control)?;
    ensure!(
        registry.catalog_state()?.is_none(),
        "rebuild does not support catalog Project, context and grant transfer yet; original state is preserved"
    );
    let namespace = registry.incarnation()?;
    let mut after = None;
    let mut revision = None;
    let mut indexes = Vec::new();
    loop {
        let page = registry.list(after, revision, 32)?;
        revision = Some(page.revision);
        for project in page.projects {
            ensure!(
                indexes.len() < MAX_INDEXES,
                "rebuild index roster exceeds bound"
            );
            let registration = project.registration;
            ensure!(
                !registry.has_unsettled_configuration(registration.binding)?,
                "rebuild requires terminal saved configuration and model-setup outcomes"
            );
            let data = Path::new(&registration.data_root);
            airk_store::service::data_scope::check(data, &control, namespace)?;
            // These separately owned settings cannot silently acquire defaults
            // in a new namespace. Their support requires their own native
            // transfer; preserve old state and refuse the option meanwhile.
            require_absent(&data.join(".quota-1.4/policy.json"), "custom quota policy")?;
            require_absent(&data.join("models/local"), "legacy local model selection")?;
            let collection =
                Collection::existing(data, &registration.root, &registration.collection)?;
            ensure!(
                collection.required_source_policy().is_none(),
                "rebuild does not support permanent source policy yet"
            );
            ensure!(
                collection.required_split()?.is_none(),
                "rebuild does not support split collection ownership"
            );
            let store = collection.selected_store()?;
            require_absent(
                &store
                    .managed_collection_directory()?
                    .join("usage-policy.json"),
                "explicit usage policy",
            )?;
            require_absent(
                &store.directory().join("index.sqlite3"),
                "historical standalone settings",
            )?;
            drop(store);
            let captured = collection.capture_saved_settings()?;
            ensure!(
                !captured.has_saved_search_policy(),
                "rebuild does not support a saved semantic search policy yet"
            );
            if let Some(applied) = captured.applied_configuration() {
                ensure!(
                    applied.source_policy_digest.is_none() && applied.retained_profile.is_none(),
                    "rebuild does not support applied source policy or retained profiles yet"
                );
            }
            let watch = registry.watch(registration.binding)?;
            ensure!(
                watch.project.registration == registration
                    && watch.project.paused == project.paused,
                "saved rebuild registration changed"
            );
            let saved_files = captured
                .files()
                .map(|(name, bytes)| (name.to_owned(), bytes.map(<[u8]>::to_vec)))
                .collect::<BTreeMap<_, _>>();
            let choices = supported_choices(
                &saved_files,
                captured
                    .applied_configuration()
                    .map(|configuration| configuration.index_detail),
                Some(&control),
                &registration.root,
            )?;
            indexes.push(SavedIndex {
                registration,
                paused: project.paused,
                watch: watch.observation.enabled,
                roots: choices.roots,
                vectors_enabled: choices.vectors_enabled,
                index_detail: choices.index_detail,
                incarnation: captured.incarnation(),
                settings_digest: captured.digest(),
                default_model: choices.default_model,
                saved_files,
            });
        }
        after = page.after;
        if after.is_none() {
            break;
        }
    }
    let revision = revision.context("missing saved registry revision")?;
    registry.list(None, Some(revision), 1)?;
    let roster = SavedRoster {
        namespace,
        revision,
        indexes,
    };
    roster.validate()?;
    Ok(roster)
}

fn require_absent(path: &Path, setting: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
        Ok(_) => anyhow::bail!("rebuild does not support {setting}; original state is preserved"),
    }
}

struct Choices {
    roots: BTreeMap<String, String>,
    vectors_enabled: bool,
    index_detail: IndexDetail,
    default_model: Option<ModelObjectSet>,
}

fn supported_choices(
    files: &BTreeMap<String, Option<Vec<u8>>>,
    applied_detail: Option<IndexDetail>,
    control: Option<&Path>,
    owner_root: &str,
) -> Result<Choices> {
    let file = |name: &str| files.get(name).and_then(Option::as_deref);
    ensure!(
        file("compression-policy.json").is_none(),
        "rebuild does not support explicit compression settings yet"
    );
    ensure!(
        file("scan-limits.json").is_none(),
        "rebuild does not support explicit scan limits yet"
    );
    let roots = match file("roots.json") {
        Some(bytes) => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Roots {
                schema: u32,
                owner_root: String,
                roots: BTreeMap<String, String>,
            }
            let roots: Roots = serde_json::from_slice(bytes)?;
            ensure!(
                roots.schema == 1 && roots.owner_root == owner_root,
                "unsupported saved roots"
            );
            roots.roots
        }
        None => BTreeMap::new(),
    };
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Envelope {
        version: u32,
        owner_root: String,
        policy: serde_json::Value,
    }
    let payload = |bytes: &[u8]| -> Result<serde_json::Value> {
        let envelope: Envelope = serde_json::from_slice(bytes)?;
        ensure!(
            envelope.version == 1 && envelope.owner_root == owner_root,
            "unsupported saved policy envelope"
        );
        Ok(envelope.policy)
    };
    let index_detail = match file("extraction-policy.json") {
        Some(bytes) => {
            let policy: airk_kernel::extraction::ExtractionPolicy =
                serde_json::from_value(payload(bytes)?)?;
            ensure!(
                matches!(policy.version, 1..=3)
                    && (policy.detail.is_none() || policy.version == 3)
                    && policy.rules.is_empty()
                    && policy.retained_profile.is_none()
                    && policy.legacy_fallback_detail.is_none(),
                "rebuild does not support custom extraction rules or retained profiles yet"
            );
            policy.legacy_detail()
        }
        None => applied_detail.context(
            "rebuild requires readable saved applied detail when extraction policy is absent",
        )?,
    };
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Vectors {
        enabled: bool,
        #[serde(default)]
        model: Option<ModelObjectSet>,
        #[serde(default)]
        selection: Option<airk_core::vector_selection::VectorSelectionPolicy>,
    }
    let vectors = match file("vector-policy.json") {
        Some(bytes) => serde_json::from_value::<Vectors>(payload(bytes)?)?,
        None => Vectors {
            enabled: true,
            model: None,
            selection: None,
        },
    };
    ensure!(
        vectors.selection.is_none(),
        "rebuild does not support custom vector selection yet"
    );
    if let Some(model) = &vectors.model {
        model.validate()?;
        if let Some(control) = control {
            let owner = airk_store::service::model_files::model_scope(control)?;
            ensure!(
                model.local_set() == crate::bundled_assets::default_plan()?
                    && model
                        .shared
                        .as_ref()
                        .is_some_and(|shared| owner == shared.owner),
                "rebuild does not support a custom, foreign or different offline model"
            );
        } else {
            // Historical observations retain their admitted model identity even
            // when a later runtime bundles another default. This syntax check
            // grants no authority to apply it to a replacement collection.
            ensure!(
                model.shared.is_some(),
                "saved rebuild model lacks native ownership"
            );
        }
    }
    Ok(Choices {
        roots,
        vectors_enabled: vectors.enabled,
        index_detail,
        default_model: vectors.model,
    })
}

/// Apply the supported saved choices only to the exact fresh registration after
/// canonical native setup, before accepting its first Index job or enabling watch.
pub(super) fn apply_saved_policy(
    control: &Path,
    registration: &ProjectRegistration,
    saved: &SavedIndex,
) -> Result<()> {
    saved.validate()?;
    if let Some(model) = &saved.default_model {
        ensure!(
            model.local_set() == crate::bundled_assets::default_plan()?,
            "rebuild target does not support the admitted bundled model"
        );
    }
    registration.validate()?;
    ensure!(
        registration.root == saved.registration.root
            && registration.collection == saved.registration.collection
            && !Path::new(&registration.data_root).starts_with(&saved.registration.data_root)
            && !Path::new(&saved.registration.data_root).starts_with(&registration.data_root),
        "rebuild policy requires matching sources and fresh storage"
    );
    let registry = ServiceRegistry::open_saved(control)?;
    let registered = registry.get(registration.binding)?;
    ensure!(
        registered.registration == *registration && registered.paused,
        "rebuild policy requires its paused native registration"
    );
    airk_store::service::data_scope::check(
        Path::new(&registration.data_root),
        control,
        registry.incarnation()?,
    )?;
    let kernel = airk_kernel::Kernel::open_service_collection_for_diagnostics(
        &registration.root,
        Some(registration.data_root.clone().into()),
        &registration.collection,
    )?;
    let kernel = crate::bundled_assets::bind_service_models(kernel, control)?;
    kernel.configure_index_detail(saved.index_detail)?;
    kernel.configure_vectors(saved.vectors_enabled)?;
    Ok(())
}

#[cfg(test)]
#[path = "runtime_rebuild_settings_tests.rs"]
mod tests;
