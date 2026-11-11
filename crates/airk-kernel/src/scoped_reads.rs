// SPDX-License-Identifier: Apache-2.0
//! Inert session authority; normal Kernel construction retains legacy reads.
use crate::Kernel;
use airk_core::{
    grant_provenance::{GrantProvenance, PersistedGrant},
    identity::{Sha256Digest, framed_digest},
    index_catalog::{
        IndexId, MAX_SELECTION, MAX_SOURCE_ROOTS, QueryProjectId, ReviewedGrant, unique_bounded,
    },
    scoped_reads::{
        CompiledPathAtom, CompiledReadScope, CompiledSourceAtom, ScopeReadRefused, SourceDescriptor,
    },
};
use airk_store::current::{CurrentRead, CurrentToken};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

pub const MAX_SCOPED_AUTHORITY_BYTES: usize = 48 * 1024;

#[cfg(test)]
fn full_index_metadata_allowed(
    scope: &CompiledReadScope,
    primary: &str,
    aliases: &BTreeMap<String, String>,
    configured: Option<&BTreeMap<String, String>>,
) -> bool {
    configured.is_none_or(|configured| configured == aliases)
        && scope.covers_sources(primary, aliases)
}

/// Validated by the native host against live catalog/policy before admission.
/// This frozen payload alone is neither a bearer credential nor live authority.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedReadAuthority {
    pub schema: u32,
    pub backing: airk_core::service::ProjectBinding,
    pub index_id: IndexId,
    pub project_ids: Vec<QueryProjectId>,
    pub grant: ReviewedGrant,
    pub provenance: GrantProvenance,
    pub sources: Vec<SourceDescriptor>,
}
impl ScopedReadAuthority {
    fn observation(
        &self,
        fingerprint: Sha256Digest,
    ) -> Result<airk_core::scoped_reads::ScopeObservation> {
        use airk_core::scoped_reads::*;
        let selected = self
            .grant
            .memberships
            .iter()
            .filter(|member| {
                member.membership.index_id == self.index_id
                    && self.project_ids.contains(&member.membership.project_id)
            })
            .collect::<Vec<_>>();
        let first = selected.first().ok_or(ScopeReadRefused)?;
        let mut memberships = selected
            .iter()
            .map(|member| ScopeMembershipObservation {
                project_id: member.membership.project_id,
                membership_revision: member.membership.revision,
            })
            .collect::<Vec<_>>();
        memberships.sort_by_key(|member| member.project_id);
        let mut source_root_ids = self
            .sources
            .iter()
            .map(|source| source.source_root_id)
            .collect::<Vec<_>>();
        source_root_ids.sort();
        let observation = ScopeObservation {
            schema: 1,
            qualifier: ScopeQualifier::SelectedSubset,
            fingerprint,
            provenance: self.provenance.kind(),
            grant: ScopeGrantObservation {
                id: self.grant.id,
                revision: self.grant.revision,
            },
            members: vec![ScopeMemberObservation {
                index_id: self.index_id,
                memberships,
                source_root_ids,
                source_revision: first.source_revision,
                policy_epoch: first.policy_epoch,
                policy_digest: first.policy_digest,
            }],
        };
        observation.validate()?;
        Ok(observation)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == 1 && !self.grant.revoked, ScopeReadRefused);
        PersistedGrant {
            grant: self.grant.clone(),
            provenance: self.provenance.clone(),
        }
        .validate()?;
        unique_bounded(&self.project_ids, MAX_SELECTION)?;
        ensure!(
            !self.project_ids.is_empty() && self.sources.len() <= MAX_SOURCE_ROOTS,
            ScopeReadRefused
        );
        let mut ids = BTreeSet::new();
        let mut roots = BTreeSet::new();
        for source in &self.sources {
            ensure!(
                ids.insert(source.source_root_id)
                    && roots.insert(&source.original_root)
                    && source.original_root.len() <= 4096
                    && std::path::Path::new(&source.original_root).is_absolute(),
                ScopeReadRefused
            );
        }
        let mut selected = Vec::new();
        let mut used_sources = BTreeSet::new();
        for project in &self.project_ids {
            let member = self
                .grant
                .memberships
                .iter()
                .find(|m| {
                    m.membership.index_id == self.index_id && m.membership.project_id == *project
                })
                .ok_or(ScopeReadRefused)?;
            ensure!(
                member
                    .membership
                    .scope
                    .iter()
                    .all(|a| ids.contains(&a.source_root_id)),
                ScopeReadRefused
            );
            used_sources.extend(
                member
                    .membership
                    .scope
                    .iter()
                    .map(|atom| atom.source_root_id),
            );
            selected.push(member);
        }
        ensure!(ids == used_sources, ScopeReadRefused);
        if let Some(first) = selected.first() {
            ensure!(
                selected
                    .iter()
                    .all(|m| m.source_revision == first.source_revision
                        && m.policy_epoch == first.policy_epoch
                        && m.policy_digest == first.policy_digest),
                ScopeReadRefused
            );
        }
        airk_core::index_catalog::encoded_size(self, MAX_SCOPED_AUTHORITY_BYTES)?;
        Ok(())
    }
    pub fn fingerprint(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(framed_digest(
            b"airk/scoped-read-authority/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
    fn compile(
        &self,
        primary: &str,
        aliases: &BTreeMap<String, String>,
    ) -> Result<CompiledReadScope> {
        let fingerprint = self.fingerprint()?;
        let mut roots = BTreeMap::new();
        for source in &self.sources {
            let prefix = if source.original_root == primary {
                String::new()
            } else {
                let mut matching = aliases
                    .iter()
                    .filter(|(_, root)| **root == source.original_root);
                let (alias, _) = matching.next().ok_or(ScopeReadRefused)?;
                ensure!(matching.next().is_none(), ScopeReadRefused);
                format!("@{alias}")
            };
            roots.insert(source.source_root_id, prefix);
        }
        let mut atoms = BTreeSet::new();
        let mut source_atoms = BTreeSet::new();
        for member in &self.grant.memberships {
            if member.membership.index_id != self.index_id
                || !self.project_ids.contains(&member.membership.project_id)
            {
                continue;
            }
            for atom in &member.membership.scope {
                let prefix = roots.get(&atom.source_root_id).ok_or(ScopeReadRefused)?;
                // A primary-root atom cannot acquire an external source merely
                // because its relative spelling matches a published alias.
                if prefix.is_empty() {
                    ensure!(
                        !aliases.keys().any(|alias| {
                            atom.path.strip_prefix('@').is_some_and(|path| {
                                path == alias
                                    || path
                                        .strip_prefix(alias.as_str())
                                        .is_some_and(|tail| tail.starts_with('/'))
                            })
                        }),
                        ScopeReadRefused
                    );
                }
                let path = match (prefix.is_empty(), atom.path.is_empty()) {
                    (true, _) => atom.path.clone(),
                    (false, true) => prefix.clone(),
                    (false, false) => format!("{prefix}/{}", atom.path),
                };
                atoms.insert((atom.kind, path));
                let original = self
                    .sources
                    .iter()
                    .find(|source| source.source_root_id == atom.source_root_id)
                    .ok_or(ScopeReadRefused)?;
                source_atoms.insert((original.original_root.clone(), atom.kind, atom.path.clone()));
            }
        }
        let result = CompiledReadScope {
            fingerprint,
            atoms: atoms
                .into_iter()
                .map(|(kind, path)| CompiledPathAtom { kind, path })
                .collect(),
            source_atoms: source_atoms
                .into_iter()
                .map(|(original_root, kind, path)| CompiledSourceAtom {
                    original_root,
                    kind,
                    path,
                })
                .collect(),
            external_aliases: aliases.keys().cloned().collect(),
        };
        result.validate()?;
        Ok(result)
    }
}

pub(crate) struct SessionScope {
    authority: ScopedReadAuthority,
    fingerprint: Sha256Digest,
    observation: airk_core::scoped_reads::ScopeObservation,
    observation_value: serde_json::Value,
    revoked: Arc<AtomicBool>,
    compiled: Mutex<Option<(CurrentToken, Arc<CompiledReadScope>, bool)>>,
    #[cfg(test)]
    reader_attachments: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    service_source_checks: std::sync::atomic::AtomicUsize,
}
impl SessionScope {
    fn live(&self) -> Result<()> {
        ensure!(!self.revoked.load(Ordering::Acquire), ScopeReadRefused);
        Ok(())
    }
    pub(crate) fn attach(&self, read: &CurrentRead) -> Result<()> {
        self.live()?;
        #[cfg(test)]
        self.reader_attachments.fetch_add(1, Ordering::Relaxed);
        let token = read.token()?;
        let mut cache = self.compiled.lock().map_err(|_| ScopeReadRefused)?;
        let scope = if let Some((cached_token, scope, _)) = cache
            .as_ref()
            .filter(|(cached_token, _, _)| *cached_token == token)
        {
            let _ = cached_token;
            scope.clone()
        } else {
            let state = read.collection_state()?.ok_or(ScopeReadRefused)?;
            let scope = Arc::new(
                self.authority
                    .compile(&read.head().scope.root, &state.configuration.roots)?,
            );
            let full_index =
                scope.covers_sources(&read.head().scope.root, &state.configuration.roots);
            *cache = Some((token, scope.clone(), full_index));
            scope
        };
        read.attach_read_scope_fence(scope, self.revoked.clone())?;
        self.live()
    }
    fn require_index_metadata(
        &self,
        read: &CurrentRead,
        configured: Option<&BTreeMap<String, String>>,
    ) -> Result<()> {
        self.live()?;
        read.token()?;
        ensure!(
            read.read_scope_fingerprint() == Some(self.fingerprint) && !read.restricts_paths(),
            ScopeReadRefused
        );
        if let Some(configured) = configured {
            let state = read.collection_state()?.ok_or(ScopeReadRefused)?;
            ensure!(*configured == state.configuration.roots, ScopeReadRefused);
        }
        self.live()
    }
}
impl Kernel {
    /// Constructs a separate immutable session; the administrative Kernel is untouched.
    /// Activation and live grant checks remain the native host's responsibility.
    pub fn with_scoped_reads(&self, authority: ScopedReadAuthority) -> Result<Self> {
        ensure!(self.read_scope.is_none(), ScopeReadRefused);
        let fingerprint = authority.fingerprint()?;
        let binding = airk_core::service::ProjectRegistration::new(
            self.root_id(),
            self.data_root.to_str().ok_or(ScopeReadRefused)?.into(),
            self.collection.clone(),
        )?
        .binding;
        ensure!(authority.backing == binding, ScopeReadRefused);
        let observation = authority.observation(fingerprint)?;
        let observation_value = serde_json::to_value(&observation)?;
        let mut session = self.clone();
        session.read_scope = Some(Arc::new(SessionScope {
            authority,
            fingerprint,
            observation,
            observation_value,
            revoked: Arc::new(AtomicBool::new(false)),
            compiled: Mutex::new(None),
            #[cfg(test)]
            reader_attachments: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            service_source_checks: std::sync::atomic::AtomicUsize::new(0),
        }));
        Ok(session)
    }
    pub fn revoke_scoped_reads(&self) {
        if let Some(scope) = &self.read_scope {
            scope.revoked.store(true, Ordering::Release);
        }
    }
    #[cfg(test)]
    pub(crate) fn note_service_source_check(&self) {
        if let Some(scope) = &self.read_scope {
            scope.service_source_checks.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub fn require_unrestricted_read(&self) -> Result<()> {
        ensure!(self.read_scope.is_none(), ScopeReadRefused);
        Ok(())
    }
    /// Complete current-index aggregates require frozen authority for every
    /// published source. The legacy path performs no additional metadata read.
    pub(crate) fn require_index_metadata(&self, read: &CurrentRead) -> Result<()> {
        if let Some(scope) = &self.read_scope {
            scope.require_index_metadata(read, None)?;
        }
        Ok(())
    }
    pub(crate) fn require_configured_index_metadata(
        &self,
        read: &CurrentRead,
        configured: &BTreeMap<String, String>,
    ) -> Result<()> {
        if let Some(scope) = &self.read_scope {
            scope.require_index_metadata(read, Some(configured))?;
        }
        Ok(())
    }
    pub fn scoped_read_fingerprint(&self) -> Result<Option<Sha256Digest>> {
        self.read_scope
            .as_ref()
            .map(|s| {
                s.live()?;
                Ok(s.fingerprint)
            })
            .transpose()
    }
    /// Frozen installed authorization identity, independent of live admission
    /// and publication. Safe to retain with refusals; never renews authority.
    pub fn scope_observation(&self) -> Option<&airk_core::scoped_reads::ScopeObservation> {
        self.read_scope.as_ref().map(|scope| &scope.observation)
    }
    pub fn observe_scoped_error(&self, error: anyhow::Error) -> anyhow::Error {
        let Some(observation) = self.scope_observation() else {
            return error;
        };
        if error
            .downcast_ref::<airk_core::scoped_reads::ScopeErrorObservation>()
            .is_some_and(|scope| scope.observation == *observation)
        {
            return error;
        }
        let message = error.to_string();
        error.context(airk_core::scoped_reads::ScopeErrorObservation {
            observation: observation.clone(),
            message,
        })
    }
    /// Shared canonical response decoration used by native/MCP/Workbench
    /// adapters before verification allocation and final response framing.
    pub fn annotate_scoped_read(&self, value: &mut serde_json::Value) -> Result<()> {
        let Some(scope) = &self.read_scope else {
            return Ok(());
        };
        let object = value.as_object_mut().ok_or(ScopeReadRefused)?;
        if let Some(existing) = object.get("scope_observation") {
            ensure!(existing == &scope.observation_value, ScopeReadRefused);
        } else {
            object.insert("scope_observation".into(), scope.observation_value.clone());
        }
        let required_bytes = serde_json::to_vec(&serde_json::to_string(value)?)?
            .len()
            .saturating_add(2048);
        ensure!(
            required_bytes <= airk_core::MAX_RESPONSE_BYTES,
            airk_core::scoped_reads::ScopeReplyBudget {
                required_bytes,
                maximum_bytes: airk_core::MAX_RESPONSE_BYTES
            }
        );
        Ok(())
    }
    pub(crate) fn cached_restricted_metadata(
        &self,
        expected: airk_core::identity::RevisionToken,
    ) -> Result<bool> {
        let Some(scope) = &self.read_scope else {
            return Ok(false);
        };
        scope.live()?;
        let cache = scope.compiled.lock().map_err(|_| ScopeReadRefused)?;
        Ok(cache.as_ref().is_some_and(|(token, _, full)| {
            crate::current::runtime::revision(*token) == expected && !full
        }))
    }
    pub(crate) fn scope_bound_cursor_identity(&self, original: &str) -> Result<String> {
        Ok(match self.scoped_read_fingerprint()? {
            None => original.to_owned(),
            Some(scope) => framed_digest(
                b"airk/scoped-cursor/1",
                &[original.as_bytes(), scope.as_bytes()],
            )
            .to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{identity::Incarnation, index_catalog::*};

    #[test]
    fn canonical_scope_observation_is_cached_compact_and_budgeted_without_reading() -> Result<()> {
        let source = tempfile::tempdir()?;
        let kernel = Kernel::open(source.path(), Some(source.path().join("state")))?;
        let mut authority = fixture(&kernel.root_id());
        authority.backing = airk_core::service::ProjectRegistration::new(
            kernel.root_id(),
            kernel.data_root.to_str().unwrap().into(),
            kernel.collection.clone(),
        )?
        .binding;
        let scoped = kernel.with_scoped_reads(authority)?;
        let before = serde_json::json!({"publication":{"token":"original-token"},"read":{"validator":"original-validator"},"paths":["public/a.rs"]});
        let mut legacy = before.clone();
        kernel.annotate_scoped_read(&mut legacy)?;
        assert_eq!(legacy, before);
        let mut result = before.clone();
        scoped.annotate_scoped_read(&mut result)?;
        let observation = result["scope_observation"].clone();
        assert_eq!(observation["qualifier"], "selected_subset");
        assert_eq!(
            observation,
            serde_json::to_value(scoped.scope_observation())?
        );
        assert!(
            !observation
                .to_string()
                .contains(kernel.root().to_str().unwrap())
        );
        assert!(observation.get("publication").is_none());
        assert_eq!(result["publication"], before["publication"]);
        assert_eq!(result["read"], before["read"]);
        scoped.annotate_scoped_read(&mut result)?;
        assert_eq!(result["scope_observation"], observation);
        assert_eq!(
            scoped
                .read_scope
                .as_ref()
                .unwrap()
                .reader_attachments
                .load(Ordering::Relaxed),
            0
        );
        let mut oversized =
            serde_json::json!({"hits":[{"text":"x".repeat(airk_core::MAX_RESPONSE_BYTES)}]});
        let error = scoped.annotate_scoped_read(&mut oversized).unwrap_err();
        let budget = error
            .downcast_ref::<airk_core::scoped_reads::ScopeReplyBudget>()
            .unwrap();
        assert!(budget.required_bytes > budget.maximum_bytes);
        let request = serde_json::from_value(
            serde_json::json!({"schema":"airk.parse.request/1","mode":"execute","verification":"off","plan":{"steps":[{"id":"help","tool":"help","args":{}}]},"output":{"view":"raw","max_bytes":1024}}),
        )?;
        let error = scoped.evidence_plan(request).unwrap_err();
        assert!(error.is::<airk_core::scoped_reads::ScopeReplyBudget>());
        assert!(
            error
                .downcast_ref::<airk_core::scoped_reads::ScopeErrorObservation>()
                .is_some()
        );
        scoped.revoke_scoped_reads();
        assert_eq!(
            serde_json::to_value(scoped.scope_observation())?,
            observation,
            "a refusal can retain installed identity without renewing authority"
        );
        Ok(())
    }

    #[test]
    fn scoped_full_status_never_probes_new_unpublished_source_roots() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("source");
        let pending = directory.path().join("pending");
        std::fs::create_dir(&source)?;
        std::fs::create_dir(&pending)?;
        std::fs::write(source.join("main.rs"), "pub struct Main;\n")?;
        let kernel = Kernel::open_service_collection(
            &source,
            Some(directory.path().join("data")),
            "availability-scope",
        )?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let mut authority = fixture(&kernel.root_id());
        authority.backing = airk_core::service::ProjectRegistration::new(
            kernel.root_id(),
            kernel.data_root.to_str().unwrap().into(),
            kernel.collection.clone(),
        )?
        .binding;
        let scoped = kernel.with_scoped_reads(authority)?;
        kernel.add_root("pending", &pending)?;
        std::fs::remove_dir(&pending)?;
        assert!(
            kernel.require_service_sources().is_err(),
            "legacy availability still checks configured pending roots"
        );
        let result = scoped.status()?;
        assert_eq!(result.files, 1);
        assert!(result.source_availability.is_none());
        assert!(result.source_roots.keys().all(|alias| alias != "pending"));
        assert_eq!(
            scoped
                .read_scope
                .as_ref()
                .unwrap()
                .service_source_checks
                .load(Ordering::Relaxed),
            0,
            "scoped status must not enter the mutable-root probe gateway"
        );
        assert_eq!(scoped.status_summary()?["files"], 1);
        assert_eq!(
            scoped
                .read_scope
                .as_ref()
                .unwrap()
                .service_source_checks
                .load(Ordering::Relaxed),
            0
        );
        Ok(())
    }
    #[test]
    fn full_index_metadata_requires_every_frozen_source_and_matching_configuration() {
        let mut scope = CompiledReadScope {
            fingerprint: Sha256Digest::from_bytes([1; 32]),
            atoms: vec![
                CompiledPathAtom {
                    kind: PathScope::Subtree,
                    path: String::new(),
                },
                CompiledPathAtom {
                    kind: PathScope::Subtree,
                    path: "@docs".into(),
                },
            ],
            external_aliases: vec!["docs".into()],
            source_atoms: vec![CompiledSourceAtom {
                original_root: "primary".into(),
                kind: PathScope::Subtree,
                path: String::new(),
            }],
        };
        let aliases = BTreeMap::from([("docs".into(), "external".into())]);
        assert!(!full_index_metadata_allowed(
            &scope, "primary", &aliases, None
        ));
        scope.source_atoms.push(CompiledSourceAtom {
            original_root: "external".into(),
            kind: PathScope::Subtree,
            path: String::new(),
        });
        assert!(full_index_metadata_allowed(
            &scope,
            "primary",
            &aliases,
            Some(&aliases)
        ));
        let mut changed = aliases.clone();
        changed.insert("later".into(), "new-source".into());
        assert!(!full_index_metadata_allowed(
            &scope, "primary", &changed, None
        ));
        changed = aliases.clone();
        changed.insert("docs".into(), "rebound-source".into());
        assert!(!full_index_metadata_allowed(
            &scope, "primary", &changed, None
        ));
        assert!(!full_index_metadata_allowed(
            &scope,
            "primary",
            &aliases,
            Some(&changed)
        ));
        scope.source_atoms[0].path = "public".into();
        assert!(!full_index_metadata_allowed(
            &scope, "primary", &aliases, None
        ));
        scope.source_atoms[0].path.clear();
        scope.source_atoms[0].kind = PathScope::Exact;
        assert!(!full_index_metadata_allowed(
            &scope, "primary", &aliases, None
        ));
    }

    #[test]
    fn current_metadata_handlers_accept_full_source_and_refuse_subset_or_new_alias() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("source");
        let external = dir.path().join("external");
        std::fs::create_dir(&root)?;
        std::fs::create_dir(&external)?;
        std::fs::write(root.join("main.rs"), "pub struct Main;\n")?;
        std::fs::write(external.join("external.rs"), "pub struct External;\n")?;
        let admin = Kernel::open(&root, Some(dir.path().join("data")))?;
        admin.configure_vectors(false)?;
        admin.index()?;
        let mut authority = fixture(&admin.root_id());
        authority.backing = airk_core::service::ProjectRegistration::new(
            admin.root_id(),
            admin.data_root.to_str().unwrap().into(),
            admin.collection.clone(),
        )?
        .binding;
        let full = admin.with_scoped_reads(authority.clone())?;
        assert!(!full.current_reader()?.restricts_paths());
        assert_eq!(full.status_summary()?["files"], 1);
        assert_eq!(full.status()?.files, 1);
        assert!(full.roots()?.indexed.is_empty());
        let token = full.status()?.publication.unwrap().token;
        assert!(full.readiness_observation(token)?.is_some());
        let query = airk_core::SearchQuery {
            text: "Main".into(),
            path_prefix: None,
            limit: 10,
            max_bytes: 4096,
        };
        let before = full
            .read_scope
            .as_ref()
            .unwrap()
            .reader_attachments
            .load(Ordering::Relaxed);
        let result = full.search(&query)?;
        assert_eq!(
            full.read_scope
                .as_ref()
                .unwrap()
                .reader_attachments
                .load(Ordering::Relaxed),
            before + 1
        );
        assert_eq!(result.publication.as_ref().unwrap().token, token);
        assert!(
            result.semantic_readiness.is_some() && result.discovery_readiness.is_some(),
            "full search carries exact-reader metadata, so rendering needs no observation reader"
        );
        authority.grant.memberships[0].membership.scope[0].path = "public".into();
        let subset = admin.with_scoped_reads(authority)?;
        assert!(subset.current_reader()?.restricts_paths());
        let result = subset.search(&query)?;
        assert!(result.semantic_readiness.is_none() && result.discovery_readiness.is_none());
        let before = subset
            .read_scope
            .as_ref()
            .unwrap()
            .reader_attachments
            .load(Ordering::Relaxed);
        assert!(subset.readiness_observation(token)?.is_none());
        assert!(subset.discovery_observation(token)?.is_none());
        assert_eq!(
            subset
                .read_scope
                .as_ref()
                .unwrap()
                .reader_attachments
                .load(Ordering::Relaxed),
            before,
            "optional restricted metadata performs no extra reader open"
        );
        assert!(
            subset
                .status_summary()
                .unwrap_err()
                .is::<ScopeReadRefused>()
        );
        assert!(subset.roots().unwrap_err().is::<ScopeReadRefused>());
        admin.add_root("docs", &external)?;
        assert!(
            full.configured_roots()
                .unwrap_err()
                .is::<ScopeReadRefused>()
        );
        assert!(full.roots().unwrap_err().is::<ScopeReadRefused>());
        admin.index()?;
        assert!(full.current_reader()?.restricts_paths());
        assert!(full.status_summary().unwrap_err().is::<ScopeReadRefused>());
        assert!(admin.status_summary().is_ok());
        Ok(())
    }
    fn fixture(root: &str) -> ScopedReadAuthority {
        let project = QueryProjectId::from_identity(Incarnation::from_bytes([1; 16]));
        let index = IndexId::from_identity(Incarnation::from_bytes([2; 16]));
        let source = SourceRootId::from_identity(Incarnation::from_bytes([3; 16]));
        ScopedReadAuthority {
            schema: 1,
            backing: Sha256Digest::from_bytes([6; 32]),
            index_id: index,
            project_ids: vec![project],
            provenance: GrantProvenance::UserReviewedProjectGrant,
            grant: ReviewedGrant {
                id: GrantId::from_identity(Incarnation::from_bytes([4; 16])),
                host: Sha256Digest::from_bytes([5; 32]),
                revision: CatalogRevision::INITIAL,
                revoked: false,
                memberships: vec![GrantedMembership {
                    membership: Membership {
                        project_id: project,
                        index_id: index,
                        revision: CatalogRevision::INITIAL,
                        scope: vec![ScopeAtom {
                            source_root_id: source,
                            kind: PathScope::Subtree,
                            path: String::new(),
                        }],
                    },
                    source_revision: CatalogRevision::INITIAL,
                    policy_epoch: 0,
                    policy_digest: None,
                }],
            },
            sources: vec![SourceDescriptor {
                source_root_id: source,
                original_root: root.into(),
            }],
        }
    }
    #[test]
    fn compiler_preserves_source_identity_and_exact_scope() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let original = dir.path().to_str().unwrap();
        let mut authority = fixture(original);
        let compiled = authority.compile(original, &BTreeMap::new())?;
        assert!(compiled.allows_path("lib.rs"));
        assert!(compiled.allows_path("@notes.md"));
        assert!(compiled.allows_path("@types/file.ts"));
        assert!(compiled.allows_source(original, "lib.rs"));
        assert!(!compiled.allows_source("different-root", "lib.rs"));
        let published = BTreeMap::from([("external".into(), "other-root".into())]);
        let primary = authority.compile(original, &published)?;
        assert!(!primary.allows_path("@external/private.rs"));
        authority.grant.memberships[0].membership.scope[0].path = "@external/private.rs".into();
        assert!(authority.compile(original, &published).is_err());
        authority.grant.memberships[0].membership.scope[0]
            .path
            .clear();
        let aliases = BTreeMap::from([("docs".into(), original.into())]);
        let aliased = authority.compile("different-primary", &aliases)?;
        assert!(aliased.allows_path("@docs/lib.rs"));
        assert!(!aliased.allows_path("lib.rs"));
        assert!(
            authority
                .compile("different-primary", &BTreeMap::new())
                .is_err()
        );
        authority.grant.memberships[0].membership.scope[0].kind = PathScope::Exact;
        authority.grant.memberships[0].membership.scope[0].path = "one.rs".into();
        let exact = authority.compile(original, &BTreeMap::new())?;
        assert!(exact.allows_path("one.rs"));
        assert!(!exact.allows_path("one.rs/child"));
        let before = authority.fingerprint()?;
        authority.grant.revision = authority.grant.revision.next()?;
        assert_ne!(before, authority.fingerprint()?);
        let before = authority.fingerprint()?;
        authority.grant.memberships[0].membership.revision = CatalogRevision::INITIAL.next()?;
        assert_ne!(before, authority.fingerprint()?);
        let before = authority.fingerprint()?;
        authority.grant.memberships[0].source_revision = CatalogRevision::INITIAL.next()?;
        assert_ne!(before, authority.fingerprint()?);
        let before = authority.fingerprint()?;
        authority.grant.memberships[0].policy_epoch = 1;
        authority.grant.memberships[0].policy_digest = Some(Sha256Digest::from_bytes([7; 32]));
        assert_ne!(before, authority.fingerprint()?);
        authority.grant.memberships[0].membership.scope.clear();
        authority.sources.clear();
        assert!(
            !authority
                .compile(original, &BTreeMap::new())?
                .allows_path("one.rs")
        );
        Ok(())
    }
    #[test]
    fn authority_provenance_is_validated_fingerprinted_and_compactly_observed() -> Result<()> {
        use airk_core::grant_provenance::{GrantProvenanceKind, native_imported_grant_id};
        let dir = tempfile::tempdir()?;
        let mut authority = fixture(dir.path().to_str().unwrap());
        let reviewed = authority.fingerprint()?;
        assert_eq!(
            authority.observation(reviewed)?.provenance,
            GrantProvenanceKind::UserReviewedProjectGrant
        );
        authority.provenance = GrantProvenance::ImportedLegacySnapshot {
            upgrade_id: Incarnation::from_bytes([8; 16]),
            plan_digest: Sha256Digest::from_bytes([9; 32]),
            legacy_binding: authority.backing,
            configuration_digest: Sha256Digest::from_bytes([10; 32]),
        };
        assert!(authority.validate().is_err());
        authority.grant.id = native_imported_grant_id(
            Incarnation::from_bytes([11; 16]),
            authority.grant.host,
            &authority.provenance,
        )?;
        let imported = authority.fingerprint()?;
        assert_ne!(reviewed, imported);
        let observation = authority.observation(imported)?;
        assert_eq!(
            observation.provenance,
            GrantProvenanceKind::ImportedLegacySnapshot
        );
        let value = serde_json::to_value(observation)?;
        assert_eq!(value["provenance"], "imported_legacy_snapshot");
        assert!(!serde_json::to_string(&value)?.contains("configuration_digest"));
        if let GrantProvenance::ImportedLegacySnapshot {
            configuration_digest,
            ..
        } = &mut authority.provenance
        {
            *configuration_digest = Sha256Digest::from_bytes([12; 32]);
        }
        assert_ne!(imported, authority.fingerprint()?);
        authority.grant.memberships[0].policy_epoch = 1;
        authority.grant.memberships[0].policy_digest = Some(Sha256Digest::from_bytes([7; 32]));
        assert!(authority.validate().is_err());
        Ok(())
    }
    #[test]
    fn authority_has_one_bounded_frame() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut authority = fixture(dir.path().to_str().unwrap());
        let source = authority.sources[0].source_root_id;
        authority.grant.memberships[0].membership.scope = (0..32)
            .map(|index| ScopeAtom {
                source_root_id: source,
                kind: PathScope::Exact,
                path: format!("{index}/{}", "x".repeat(2000)),
            })
            .collect();
        assert!(authority.validate().is_err());
        Ok(())
    }
    #[test]
    fn session_authority_is_immutable_separate_and_revocable() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("source");
        std::fs::create_dir(&root)?;
        let admin = Kernel::open(&root, Some(dir.path().join("data")))?;
        let mut authority = fixture(root.to_str().unwrap());
        assert!(admin.with_scoped_reads(authority.clone()).is_err());
        authority.backing = airk_core::service::ProjectRegistration::new(
            admin.root_id(),
            admin.data_root.to_str().unwrap().into(),
            admin.collection.clone(),
        )?
        .binding;
        let session = admin.with_scoped_reads(authority)?;
        assert!(admin.require_unrestricted_read().is_ok());
        assert!(
            session
                .require_unrestricted_read()
                .unwrap_err()
                .is::<ScopeReadRefused>()
        );
        assert!(
            session
                .with_scoped_reads(fixture(root.to_str().unwrap()))
                .is_err()
        );
        let clone = session.clone();
        session.revoke_scoped_reads();
        assert!(
            clone
                .scoped_read_fingerprint()
                .unwrap_err()
                .is::<ScopeReadRefused>()
        );
        assert_eq!(admin.scoped_read_fingerprint()?, None);
        Ok(())
    }
}
