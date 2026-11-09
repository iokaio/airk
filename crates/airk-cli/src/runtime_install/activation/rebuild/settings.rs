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

const MAX_INDEXES: usize = 64;
const MAX_ROSTER_BYTES: usize = 128 * 1024;

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
    pub incarnation: Option<Incarnation>,
    pub settings_digest: Sha256Digest,
    #[serde(
        default = "preferences_available",
        skip_serializing_if = "preferences_available_value"
    )]
    pub settings_available: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub discarded_preferences: Vec<String>,
    pub default_model: Option<ModelObjectSet>,
    /// Exact original envelope bytes and explicit absence, retained as evidence.
    /// The new native owner reapplies only the validated supported choices.
    pub saved_files: BTreeMap<String, Option<Vec<u8>>>,
}

fn preferences_available() -> bool {
    true
}
fn preferences_available_value(value: &bool) -> bool {
    *value
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
    pub fn check_clean_break(&self, control: &Path) -> Result<()> {
        self.check_observation(&capture_clean_break(control)?)
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
            self.settings_available == self.incarnation.is_some()
                && self.discarded_preferences.len() <= 16
                && self
                    .discarded_preferences
                    .iter()
                    .all(|value| !value.is_empty() && value.len() <= 256)
                && self
                    .discarded_preferences
                    .iter()
                    .collect::<BTreeSet<_>>()
                    .len()
                    == self.discarded_preferences.len(),
            "invalid rebuild saved-preference qualification"
        );
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
                incarnation: Some(captured.incarnation()),
                settings_digest: captured.digest(),
                settings_available: true,
                discarded_preferences: Vec::new(),
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

/// Capture all active saved index selections for the 1.8.16 clean break. This
/// reads registry metadata and optional preference envelopes, never an old
/// publication. Historical jobs, grants and index identities are not restored.
pub(super) fn preflight_clean_break(control: &Path) -> Result<usize> {
    let mut registry = ServiceRegistry::open_saved(control)?;
    let catalog = registry.catalog_state()?;
    let mut catalog_bindings = BTreeSet::new();
    if let Some(catalog) = &catalog {
        for (index, roots, policy) in registry.catalog_clean_break_sources(catalog.revision)? {
            ensure!(
                policy.as_ref().is_none_or(
                    |policy| policy.positive_includes.is_none() && policy.denies.is_empty()
                ),
                "clean-break source selection has include/exclude boundaries requiring explicit reconstruction; original state is preserved"
            );
            ensure!(
                roots.iter().filter(|root| root.owner).count() == 1
                    && roots.iter().all(|root| if root.owner {
                        root.canonical_path == index.backing.root && root.aliases.is_empty()
                    } else {
                        root.aliases.len() == 1
                    }),
                "clean-break cannot reconstruct multiple aliases for one source; review the saved selection before reset"
            );
            ensure!(
                catalog_bindings.insert(index.backing.binding),
                "duplicate clean-break backing identity"
            );
        }
    }
    let mut after = None;
    let mut revision = None;
    let mut bindings = BTreeSet::new();
    loop {
        let page = registry.list(after, revision, 32)?;
        revision = Some(page.revision);
        for project in page.projects {
            ensure!(
                bindings.len() < MAX_INDEXES && bindings.insert(project.registration.binding),
                "clean-break complete index roster exceeds 64 indexes or duplicates a binding"
            );
        }
        after = page.after;
        if after.is_none() {
            break;
        }
    }
    if catalog.is_some() {
        ensure!(
            bindings == catalog_bindings,
            "clean-break complete source catalog differs from active registrations"
        );
    }
    Ok(bindings.len())
}

/// Final supported preference capture after the installer proves old-owner exit.
pub(super) fn capture_clean_break(control: &Path) -> Result<SavedRoster> {
    let control = control.canonicalize()?;
    let mut registry = ServiceRegistry::open_saved(&control)?;
    let namespace = registry.incarnation()?;
    let catalog = registry.catalog_state()?;
    let mut catalog_sources = match &catalog {
        Some(catalog) => registry
            .catalog_clean_break_sources(catalog.revision)?
            .into_iter()
            .map(|(index, roots, policy)| (index.backing.binding, (index, roots, policy)))
            .collect::<BTreeMap<_, _>>(),
        None => BTreeMap::new(),
    };
    let mut after = None;
    let mut revision = None;
    let mut indexes = Vec::new();
    loop {
        let page = registry.list(after, revision, 32)?;
        revision = Some(page.revision);
        for project in page.projects {
            ensure!(
                indexes.len() < MAX_INDEXES,
                "clean-break complete index roster exceeds 64 indexes"
            );
            let registration = project.registration;
            let watch = registry.watch(registration.binding)?;
            ensure!(
                watch.project.registration == registration
                    && watch.project.paused == project.paused,
                "clean-break saved registration changed"
            );
            let roots = if catalog.is_some() {
                let (index, sources, policy) =
                    catalog_sources.remove(&registration.binding).context(
                        "clean-break active registration lacks a complete catalog source selection",
                    )?;
                ensure!(
                    index.backing == registration,
                    "clean-break catalog backing differs from registration"
                );
                ensure!(
                    policy.as_ref().is_none_or(
                        |policy| policy.positive_includes.is_none() && policy.denies.is_empty()
                    ),
                    "clean-break source selection has include/exclude boundaries requiring explicit reconstruction; original state is preserved"
                );
                let mut roots = BTreeMap::new();
                let mut owner = false;
                for source in sources {
                    if source.owner {
                        ensure!(
                            !owner
                                && source.canonical_path == registration.root
                                && source.aliases.is_empty(),
                            "clean-break owner source differs from registration"
                        );
                        owner = true;
                    } else {
                        ensure!(
                            !source.aliases.is_empty(),
                            "clean-break additional source lacks a saved alias"
                        );
                        for alias in source.aliases {
                            ensure!(
                                roots.insert(alias, source.canonical_path.clone()).is_none(),
                                "duplicate clean-break source alias"
                            );
                        }
                    }
                }
                ensure!(owner, "clean-break owner source is missing");
                Some(roots)
            } else {
                None
            };
            let data = Path::new(&registration.data_root);
            let captured = if data.try_exists()? {
                ensure!(
                    airk_store::service::data_scope::has_saved_binding(data, &control, namespace)?,
                    "clean-break storage lacks its original ownership receipt"
                );
                match Collection::existing(data, &registration.root, &registration.collection)
                    .and_then(|collection| {
                        collection.capture_clean_break_preferences(roots.is_none())
                    }) {
                    Ok(captured) => Some(captured),
                    Err(error)
                        if roots.is_some()
                            && error.downcast_ref::<std::io::Error>().is_some_and(|error| {
                                error.kind() == std::io::ErrorKind::NotFound
                            }) =>
                    {
                        None
                    }
                    Err(error) => return Err(error),
                }
            } else {
                ensure!(
                    roots.is_some(),
                    "clean-break legacy source selection is unavailable; review sources before reset"
                );
                None
            };
            let original_files = captured
                .as_ref()
                .map(|captured| {
                    captured
                        .files()
                        .map(|(name, bytes)| (name.to_owned(), bytes.map(<[u8]>::to_vec)))
                        .collect::<BTreeMap<_, _>>()
                })
                .unwrap_or_default();
            let roots = match roots {
                Some(roots) => roots,
                None => clean_break_roots(&original_files, &registration.root)?,
            };
            ensure!(
                roots.values().collect::<BTreeSet<_>>().len() == roots.len(),
                "clean-break cannot reconstruct multiple aliases for one source; review the saved selection before reset"
            );
            let (mut vectors_enabled, index_detail) =
                clean_break_preferences(&original_files, &registration.root)?;
            let mut discarded_preferences = clean_break_discarded_preferences(&original_files)?;
            if captured.is_none() {
                vectors_enabled = false;
                discarded_preferences.push("unavailable_saved_preferences".into());
                discarded_preferences.push("semantic_disabled_until_review".into());
            }
            let saved_files = normalized_clean_break_files(
                &registration.root,
                &roots,
                vectors_enabled,
                index_detail,
            )?;
            let settings_digest = match &captured {
                Some(captured) => captured.digest(),
                None => airk_core::identity::framed_digest(
                    b"airk/clean-break-preferences-unavailable/1",
                    &[registration.binding.as_bytes()],
                ),
            };
            indexes.push(SavedIndex {
                registration,
                paused: project.paused,
                watch: watch.observation.enabled,
                roots,
                vectors_enabled,
                index_detail,
                incarnation: captured.as_ref().map(|captured| captured.incarnation()),
                settings_digest,
                settings_available: captured.is_some(),
                discarded_preferences,
                default_model: None,
                saved_files,
            });
        }
        after = page.after;
        if after.is_none() {
            break;
        }
    }
    ensure!(
        catalog_sources.is_empty(),
        "clean-break catalog census contains an unobserved active backing"
    );
    let revision = revision.context("missing clean-break registry revision")?;
    registry.list(None, Some(revision), 1)?;
    if let Some(catalog) = catalog {
        ensure!(
            registry
                .catalog_state()?
                .is_some_and(|current| current.revision == catalog.revision),
            "clean-break source catalog changed"
        );
    }
    indexes.sort_by_key(|index| index.registration.binding);
    let roster = SavedRoster {
        namespace,
        revision,
        indexes,
    };
    roster.validate()?;
    Ok(roster)
}

fn clean_break_roots(
    files: &BTreeMap<String, Option<Vec<u8>>>,
    owner: &str,
) -> Result<BTreeMap<String, String>> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Roots {
        schema: u32,
        owner_root: String,
        roots: BTreeMap<String, String>,
    }
    match files.get("roots.json").and_then(Option::as_deref) {
        Some(bytes) => {
            let roots: Roots = serde_json::from_slice(bytes)?;
            ensure!(
                roots.schema == 1 && roots.owner_root == owner,
                "clean-break saved roots differ from registration"
            );
            Ok(roots.roots)
        }
        None => Ok(BTreeMap::new()),
    }
}

fn clean_break_preferences(
    files: &BTreeMap<String, Option<Vec<u8>>>,
    owner: &str,
) -> Result<(bool, IndexDetail)> {
    let policy = |name: &str| -> Result<Option<serde_json::Value>> {
        let Some(bytes) = files.get(name).and_then(Option::as_deref) else {
            return Ok(None);
        };
        let value: serde_json::Value = serde_json::from_slice(bytes)?;
        ensure!(
            value["version"] == 1 && value["owner_root"] == owner && value["policy"].is_object(),
            "clean-break saved preference envelope differs from registration"
        );
        Ok(Some(value["policy"].clone()))
    };
    let vectors = policy("vector-policy.json")?
        .map(|policy| {
            policy["enabled"]
                .as_bool()
                .context("clean-break vector preference is malformed")
        })
        .transpose()?
        .unwrap_or(true);
    let detail = policy("extraction-policy.json")?
        .and_then(|policy| {
            policy
                .get("detail")
                .filter(|detail| !detail.is_null())
                .cloned()
        })
        .map(serde_json::from_value)
        .transpose()?
        .unwrap_or_default();
    Ok((vectors, detail))
}

fn normalized_clean_break_files(
    owner: &str,
    roots: &BTreeMap<String, String>,
    vectors: bool,
    detail: IndexDetail,
) -> Result<BTreeMap<String, Option<Vec<u8>>>> {
    Ok(BTreeMap::from([
        (
            "roots.json".into(),
            Some(serde_json::to_vec(
                &serde_json::json!({"schema":1,"owner_root":owner,"roots":roots}),
            )?),
        ),
        (
            "extraction-policy.json".into(),
            Some(serde_json::to_vec(
                &serde_json::json!({"version":1,"owner_root":owner,"policy":{"version":3,"detail":detail,"rules":[]}}),
            )?),
        ),
        (
            "vector-policy.json".into(),
            Some(serde_json::to_vec(
                &serde_json::json!({"version":1,"owner_root":owner,"policy":{"enabled":vectors}}),
            )?),
        ),
        ("compression-policy.json".into(), None),
        ("scan-limits.json".into(), None),
    ]))
}

fn clean_break_discarded_preferences(
    files: &BTreeMap<String, Option<Vec<u8>>>,
) -> Result<Vec<String>> {
    let mut discarded = Vec::new();
    for (name, fields) in [
        ("vector-policy.json", &["model", "selection"][..]),
        (
            "extraction-policy.json",
            &["rules", "retained_profile", "legacy_fallback_detail"][..],
        ),
    ] {
        if let Some(bytes) = files.get(name).and_then(Option::as_deref) {
            let value: serde_json::Value = serde_json::from_slice(bytes)?;
            for field in fields {
                if value["policy"].get(field).is_some_and(|value| {
                    !value.is_null() && !value.as_array().is_some_and(Vec::is_empty)
                }) {
                    discarded.push(format!("{name}:{field}"));
                }
            }
        }
    }
    Ok(discarded)
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
mod tests;
