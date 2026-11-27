// SPDX-License-Identifier: Apache-2.0
//! Inert session authority; normal Kernel construction retains legacy reads.
use crate::Kernel;
use airk_core::{
    identity::{Sha256Digest, framed_digest},
    index_catalog::{
        CatalogRevision, IndexId, MAX_SELECTION, MAX_SOURCE_ROOTS, QueryProjectId, ScopeAtom,
        unique_bounded,
    },
    scoped_reads::{
        CompiledPathAtom, CompiledReadScope, CompiledSourceAtom, ScopeReadRefused,
        SelectionScopeRef, SourceDescriptor,
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

/// Closed selected-index metadata whitelist, not an administrative capability.
#[derive(Clone, Copy)]
pub enum ScopedIndexMetadataView {
    ExtractionPolicy,
    VectorSelection,
    VectorStatus,
    Roots,
    ScanLimits,
}
/// Internal native adapter result; the existing settings body stays canonical.
pub struct ScopedIndexMetadata {
    pub value: serde_json::Value,
    pub observation: airk_core::scoped_reads::ScopeObservation,
    pub publication: airk_core::evidence::RevisionEnvelope,
}

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

/// One compiler for every authority source: the same atoms give the same
/// path, source and alias predicates; only the fingerprint differs.
fn compile_atoms<'a>(
    fingerprint: Sha256Digest,
    sources: &[SourceDescriptor],
    selected: impl Iterator<Item = &'a ScopeAtom>,
    primary: &str,
    aliases: &BTreeMap<String, String>,
) -> Result<CompiledReadScope> {
    let mut roots = BTreeMap::new();
    for source in sources {
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
    for atom in selected {
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
        let original = sources
            .iter()
            .find(|source| source.source_root_id == atom.source_root_id)
            .ok_or(ScopeReadRefused)?;
        source_atoms.insert((original.original_root.clone(), atom.kind, atom.path.clone()));
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

/// Read authority from an in-memory W4/A2 selection grant (selection
/// authority spec §2.2). Validated by the native host against the live
/// catalog at exactly `selection.catalog_revision` before admission; this
/// frozen payload alone is neither a bearer credential nor live authority.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionReadAuthority {
    pub schema: u32,
    pub backing: airk_core::service::ProjectBinding,
    pub index_id: IndexId,
    /// The selected projects that have a membership of `index_id`; never empty.
    pub project_ids: Vec<QueryProjectId>,
    pub selection: SelectionScopeRef,
    /// Current `Membership.scope` atoms of those projects for `index_id`.
    pub atoms: Vec<(QueryProjectId, ScopeAtom)>,
    pub sources: Vec<SourceDescriptor>,
    pub membership_revisions: Vec<(QueryProjectId, CatalogRevision)>,
    pub source_revision: CatalogRevision,
    pub policy_epoch: u64,
    pub policy_digest: Option<Sha256Digest>,
}
impl SelectionReadAuthority {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == 1, ScopeReadRefused);
        unique_bounded(&self.project_ids, MAX_SELECTION)?;
        ensure!(
            !self.project_ids.is_empty() && self.sources.len() <= MAX_SOURCE_ROOTS,
            ScopeReadRefused
        );
        // Exactly one membership revision per read project, no extras.
        let revisions = self
            .membership_revisions
            .iter()
            .map(|(project, _)| *project)
            .collect::<Vec<_>>();
        unique_bounded(&revisions, MAX_SELECTION)?;
        ensure!(
            revisions.iter().collect::<BTreeSet<_>>()
                == self.project_ids.iter().collect::<BTreeSet<_>>(),
            ScopeReadRefused
        );
        // Every atom belongs to a read project; every read project has atoms,
        // validated as one membership scope.
        ensure!(
            self.atoms
                .iter()
                .all(|(project, _)| self.project_ids.contains(project)),
            ScopeReadRefused
        );
        for project in &self.project_ids {
            let scope = self
                .atoms
                .iter()
                .filter(|(owner, _)| owner == project)
                .map(|(_, atom)| atom.clone())
                .collect::<Vec<_>>();
            ensure!(!scope.is_empty(), ScopeReadRefused);
            airk_core::index_catalog::atoms(&scope)?;
        }
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
        let used = self
            .atoms
            .iter()
            .map(|(_, atom)| atom.source_root_id)
            .collect::<BTreeSet<_>>();
        ensure!(ids == used, ScopeReadRefused);
        ensure!(
            (self.policy_epoch == 0) == self.policy_digest.is_none(),
            ScopeReadRefused
        );
        // Nothing in the authority may be newer than the catalog revision the
        // selection was granted (and the authority built) at.
        let built = self.selection.catalog_revision;
        ensure!(
            self.source_revision <= built
                && self
                    .membership_revisions
                    .iter()
                    .all(|(_, revision)| *revision <= built),
            ScopeReadRefused
        );
        airk_core::index_catalog::encoded_size(self, MAX_SCOPED_AUTHORITY_BYTES)?;
        Ok(())
    }
    pub fn fingerprint(&self) -> Result<Sha256Digest> {
        self.validate()?;
        self.digest()
    }
    /// Its own domain: never equal to an open read's fingerprint.
    fn digest(&self) -> Result<Sha256Digest> {
        Ok(framed_digest(
            b"airk/scoped-read-authority/selection/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
    fn compile(
        &self,
        primary: &str,
        aliases: &BTreeMap<String, String>,
    ) -> Result<CompiledReadScope> {
        let fingerprint = self.fingerprint()?;
        compile_atoms(
            fingerprint,
            &self.sources,
            self.atoms.iter().map(|(_, atom)| atom),
            primary,
            aliases,
        )
    }
    fn observation(
        &self,
        fingerprint: Sha256Digest,
    ) -> Result<airk_core::scoped_reads::ScopeObservation> {
        use airk_core::scoped_reads::*;
        let mut memberships = self
            .membership_revisions
            .iter()
            .map(
                |(project_id, membership_revision)| ScopeMembershipObservation {
                    project_id: *project_id,
                    membership_revision: *membership_revision,
                },
            )
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
            selection: self.selection.observation(),
            members: vec![ScopeMemberObservation {
                index_id: self.index_id,
                memberships,
                source_root_ids,
                source_revision: self.source_revision,
                policy_epoch: self.policy_epoch,
                policy_digest: self.policy_digest,
            }],
        };
        observation.validate()?;
        Ok(observation)
    }
}

/// Read authority of a U105 open agent read (U106(d)): one named index read
/// whole, one Subtree `""` atom per enabled source. Validated by the native
/// host against the live catalog at exactly `selection.catalog_revision`
/// before admission; this frozen payload alone grants nothing. Contract
/// step: the host carrier exists, the worker refuses installation.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenReadAuthority {
    pub schema: u32,
    pub backing: airk_core::service::ProjectBinding,
    pub index_id: IndexId,
    /// Always owned by `SelectionOwnerKind::Agent`.
    pub selection: SelectionScopeRef,
    /// Every enabled source of the index; never empty.
    pub sources: Vec<SourceDescriptor>,
    pub source_revision: CatalogRevision,
    pub policy_epoch: u64,
    pub policy_digest: Option<Sha256Digest>,
}
impl OpenReadAuthority {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1
                && self.selection.owner == airk_core::scoped_reads::SelectionOwnerKind::Agent
                && !self.sources.is_empty()
                && self.sources.len() <= MAX_SOURCE_ROOTS
                && (self.policy_epoch == 0) == self.policy_digest.is_none()
                && self.policy_epoch <= i64::MAX as u64
                && self.source_revision <= self.selection.catalog_revision,
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
        airk_core::index_catalog::encoded_size(self, MAX_SCOPED_AUTHORITY_BYTES)?;
        Ok(())
    }
    pub fn fingerprint(&self) -> Result<Sha256Digest> {
        self.validate()?;
        self.digest()
    }
    /// Its own domain: never equal to a selection fingerprint.
    fn digest(&self) -> Result<Sha256Digest> {
        Ok(framed_digest(
            b"airk/scoped-read-authority/open/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
    fn atoms(&self) -> Vec<ScopeAtom> {
        self.sources
            .iter()
            .map(|source| ScopeAtom {
                source_root_id: source.source_root_id,
                kind: airk_core::index_catalog::PathScope::Subtree,
                path: String::new(),
            })
            .collect()
    }
    fn compile(
        &self,
        primary: &str,
        aliases: &BTreeMap<String, String>,
    ) -> Result<CompiledReadScope> {
        let fingerprint = self.fingerprint()?;
        let atoms = self.atoms();
        compile_atoms(fingerprint, &self.sources, atoms.iter(), primary, aliases)
    }
    fn observation(
        &self,
        fingerprint: Sha256Digest,
    ) -> Result<airk_core::scoped_reads::ScopeObservation> {
        use airk_core::scoped_reads::*;
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
            selection: self.selection.observation(),
            members: vec![ScopeMemberObservation {
                index_id: self.index_id,
                memberships: Vec::new(),
                source_root_ids,
                source_revision: self.source_revision,
                policy_epoch: self.policy_epoch,
                policy_digest: self.policy_digest,
            }],
        };
        observation.validate()?;
        Ok(observation)
    }
}

/// The installed authority source. Every reader goes through these accessors,
/// so path, source and alias checks, fences and annotation are shared code.
enum Authority {
    Selection(SelectionReadAuthority),
    Open(OpenReadAuthority),
}
impl Authority {
    fn backing(&self) -> airk_core::service::ProjectBinding {
        match self {
            Self::Selection(authority) => authority.backing,
            Self::Open(authority) => authority.backing,
        }
    }
    fn index_id(&self) -> IndexId {
        match self {
            Self::Selection(authority) => authority.index_id,
            Self::Open(authority) => authority.index_id,
        }
    }
    fn sources(&self) -> &[SourceDescriptor] {
        match self {
            Self::Selection(authority) => &authority.sources,
            Self::Open(authority) => &authority.sources,
        }
    }
    fn fingerprint(&self) -> Result<Sha256Digest> {
        match self {
            Self::Selection(authority) => authority.fingerprint(),
            Self::Open(authority) => authority.fingerprint(),
        }
    }
    fn observation(
        &self,
        fingerprint: Sha256Digest,
    ) -> Result<airk_core::scoped_reads::ScopeObservation> {
        match self {
            Self::Selection(authority) => authority.observation(fingerprint),
            Self::Open(authority) => authority.observation(fingerprint),
        }
    }
    fn compile(
        &self,
        primary: &str,
        aliases: &BTreeMap<String, String>,
    ) -> Result<CompiledReadScope> {
        match self {
            Self::Selection(authority) => authority.compile(primary, aliases),
            Self::Open(authority) => authority.compile(primary, aliases),
        }
    }
    /// Every selected membership is bound to exactly this source policy.
    fn policy_matches(&self, epoch: u64, digest: Option<Sha256Digest>) -> bool {
        match self {
            Self::Selection(authority) => {
                authority.policy_epoch == epoch && authority.policy_digest == digest
            }
            Self::Open(authority) => {
                authority.policy_epoch == epoch && authority.policy_digest == digest
            }
        }
    }
    /// The selected projects whose scope admits this canonical source path.
    fn attributed_projects(
        &self,
        source: airk_core::index_catalog::SourceRootId,
        path: &str,
    ) -> Vec<QueryProjectId> {
        let mut projects = match self {
            Self::Selection(authority) => authority
                .atoms
                .iter()
                .filter(|(_, atom)| atom.matches_canonical(source, path))
                .map(|(project, _)| *project)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            // U106(d): an open read reads its index whole and attributes no
            // project; `scoped_source_attribution` admits it by source alone.
            Self::Open(_) => Vec::new(),
        };
        projects.sort();
        projects
    }
}

/// Observations of one scoped metadata read while the publication keeps
/// moving under the same authority (change-log entry 67).
const METADATA_OBSERVATION_ATTEMPTS: usize = 3;
/// Private: a newer publication landed during a read-only scoped read under
/// the same authority. Never rendered; the read is observed again or becomes
/// the retryable `publication_busy`.
#[derive(Clone, Copy, Debug)]
struct PublicationMoved;
impl std::fmt::Display for PublicationMoved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the publication moved during a scoped read")
    }
}
impl std::error::Error for PublicationMoved {}

/// The retryable `publication_busy` of a source projection whose expected
/// publication was replaced under the same authority.
fn source_projection_moved() -> anyhow::Error {
    anyhow::anyhow!("a newer publication replaced the expected one")
        .context(airk_store::current::CurrentGateContention::Applying)
}

/// What a read must keep once a configuration ahead of the publication is
/// published, for `SessionScope::refusal_or_pending`.
#[derive(Clone, Copy)]
enum Coverage {
    /// An evidence read: the authority resolves.
    Authority,
    /// Index metadata: the authority also covers every source in full.
    FullIndex,
}

pub(crate) struct SessionScope {
    /// Shared with an Answer job's copy (`with_own_read_revocation`), which
    /// differs only in its revocation flag.
    authority: Arc<Authority>,
    fingerprint: Sha256Digest,
    observation: airk_core::scoped_reads::ScopeObservation,
    observation_value: serde_json::Value,
    revoked: Arc<AtomicBool>,
    compiled: Mutex<Option<(CurrentToken, Arc<CompiledReadScope>, bool)>>,
    #[cfg(test)]
    reader_attachments: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    service_source_checks: std::sync::atomic::AtomicUsize,
    /// Test seam: runs after each metadata view is computed, before the
    /// publication is checked again (change-log entry 67).
    #[cfg(test)]
    metadata_observed: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}
impl SessionScope {
    fn live(&self) -> Result<()> {
        ensure!(!self.revoked.load(Ordering::Acquire), ScopeReadRefused);
        Ok(())
    }
    /// Genesis has no published aliases. The authority is checked against
    /// the index's source configuration instead (change-log entry 67), so a
    /// multi-root scope whose roots are configured waits for the first
    /// publication like a single-root one. A root the configuration does not
    /// have either is still a definite foreign-root refusal.
    pub(crate) fn require_bootstrap_authority(
        &self,
        read: &CurrentRead,
        configured: &BTreeMap<String, String>,
    ) -> Result<()> {
        self.live()?;
        self.authority
            .compile(&read.head().scope.root, configured)?;
        self.live()
    }
    /// The published roots refused this authority. The typed retryable
    /// pending reconcile (change-log entry 67) answers only when the same
    /// request would pass once the configuration is published: the
    /// configuration differs from the published roots, the authority
    /// resolves against it and, for a read that requires the full index
    /// (`Coverage::FullIndex`), it covers every configured source the way
    /// the published-roots check would. Anything else, including a partial
    /// scope that reconciling can never lift, a revoked scope or an
    /// unresolvable one, keeps the refusal (entry 67 addendum).
    fn refusal_or_pending(
        &self,
        read: &CurrentRead,
        published: &BTreeMap<String, String>,
        configured: &BTreeMap<String, String>,
        coverage: Coverage,
    ) -> anyhow::Error {
        let primary = &read.head().scope.root;
        let passes_once_published = self.live().is_ok()
            && configured != published
            && self
                .authority
                .compile(primary, configured)
                .is_ok_and(|scope| match coverage {
                    Coverage::Authority => true,
                    Coverage::FullIndex => scope.covers_sources(primary, configured),
                });
        if passes_once_published {
            crate::retrieval_retry::ScopeReconcilePending.into()
        } else {
            ScopeReadRefused.into()
        }
    }
    pub(crate) fn attach(
        &self,
        read: &CurrentRead,
        configured: impl FnOnce() -> Result<BTreeMap<String, String>>,
    ) -> Result<()> {
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
            let compiled = self
                .authority
                .compile(&read.head().scope.root, &state.configuration.roots);
            let scope = Arc::new(match compiled {
                Ok(scope) => scope,
                Err(error) if error.is::<ScopeReadRefused>() => {
                    return Err(self.refusal_or_pending(
                        read,
                        &state.configuration.roots,
                        &configured()?,
                        Coverage::Authority,
                    ));
                }
                Err(error) => return Err(error),
            });
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
            read.read_scope_fingerprint() == Some(self.fingerprint)
                && !read.selected_scope_restricts_paths(),
            ScopeReadRefused
        );
        if let Some(configured) = configured {
            let state = read.collection_state()?.ok_or(ScopeReadRefused)?;
            // A configuration that differs from the published roots, still
            // resolves this authority and keeps it covering the full index is
            // ahead of the publication (change-log entry 67 and addendum),
            // not outside the authority.
            if *configured != state.configuration.roots {
                return Err(self.refusal_or_pending(
                    read,
                    &state.configuration.roots,
                    configured,
                    Coverage::FullIndex,
                ));
            }
        }
        self.live()
    }
}
impl Kernel {
    pub(crate) fn require_scoped_policy_binding(
        &self,
        policy: Option<&airk_core::source_policy::SourcePolicy>,
    ) -> Result<()> {
        let Some(session) = &self.read_scope else {
            return Ok(());
        };
        session.live()?;
        let expected_digest = policy.map(|policy| policy.identity()).transpose()?;
        let expected_epoch = policy.map_or(0, |policy| policy.epoch.get());
        ensure!(
            policy.is_none_or(|policy| policy.index_id == session.authority.index_id()),
            ScopeReadRefused
        );
        ensure!(
            session
                .authority
                .policy_matches(expected_epoch, expected_digest),
            ScopeReadRefused
        );
        Ok(())
    }
    pub(crate) fn policy_observation_has_full_scope(
        &self,
        policy: &airk_core::source_policy::SourcePolicy,
    ) -> bool {
        let Some(session) = &self.read_scope else {
            return true;
        };
        let aliases: BTreeMap<_, _> = policy
            .roots
            .iter()
            .filter(|root| !root.owner)
            .flat_map(|root| {
                root.aliases
                    .iter()
                    .map(|alias| (alias.clone(), root.canonical_path.clone()))
            })
            .collect();
        self.require_scoped_policy_binding(Some(policy)).is_ok()
            && session
                .authority
                .compile(&self.root_id(), &aliases)
                .is_ok_and(|scope| scope.covers_sources(&self.root_id(), &aliases))
    }
    /// Read only the selected authority's source-name projection from the exact
    /// retained publication. This never consults current source configuration,
    /// enumerates files, opens live sources or hashes their bytes.
    pub fn scoped_source_projection(
        &self,
        project: airk_core::service::ProjectBinding,
        expected: airk_core::identity::RevisionToken,
    ) -> Result<airk_core::scoped_reads::ScopedSourceProjection> {
        use airk_core::scoped_reads::{MemberSourceProjection, ScopedSourceProjection};
        let scope = self.read_scope.as_ref().ok_or(ScopeReadRefused)?;
        scope.live()?;
        ensure!(scope.authority.backing() == project, ScopeReadRefused);
        let read = self.current_reader()?;
        let publication = crate::current::runtime::envelope(&read)?;
        // A newer publication under the same authority is not a refusal
        // (change-log entry 67). The projection of `expected` can no longer
        // be read, so the caller decides whether its evidence is read again.
        if publication.token != expected {
            return Err(source_projection_moved());
        }
        let state = read.collection_state()?.ok_or(ScopeReadRefused)?;
        let primary = &read.head().scope.root;
        let mut sources = Vec::with_capacity(scope.authority.sources().len());
        for source in scope.authority.sources() {
            let is_primary = source.original_root == *primary;
            let aliases = if is_primary {
                Vec::new()
            } else {
                state
                    .configuration
                    .roots
                    .iter()
                    .filter(|(_, root)| **root == source.original_root)
                    .map(|(alias, _)| alias.clone())
                    .collect::<Vec<_>>()
            };
            ensure!(is_primary || !aliases.is_empty(), ScopeReadRefused);
            sources.push(MemberSourceProjection {
                source_root_id: source.source_root_id,
                original_root: source.original_root.clone(),
                primary: is_primary,
                aliases,
            });
        }
        sources.sort_by_key(|source| source.source_root_id);
        let projection = ScopedSourceProjection {
            publication,
            scope_fingerprint: scope.fingerprint,
            sources,
        };
        projection.validate()?;
        if crate::current::runtime::revision(read.token()?) != expected {
            return Err(source_projection_moved());
        }
        scope.live()?;
        Ok(projection)
    }
    /// Inspect only the selected index after full frozen-source coverage and
    /// current/published configuration agreement. Never clears installed scope,
    /// enumerates sources, reads an arbitrary input file, or grants administration.
    ///
    /// The read is read-only, so a publication that lands under the same
    /// authority while it runs is read again, at most
    /// `METADATA_OBSERVATION_ATTEMPTS` times; a publication still moving
    /// after that is the retryable `publication_busy` (change-log entry 67).
    pub fn scoped_index_metadata(
        &self,
        project: airk_core::service::ProjectBinding,
        view: ScopedIndexMetadataView,
    ) -> Result<ScopedIndexMetadata> {
        let mut attempts = 0;
        loop {
            attempts += 1;
            match self.scoped_index_metadata_once(project, view) {
                Err(error) if error.is::<PublicationMoved>() => {
                    if attempts < METADATA_OBSERVATION_ATTEMPTS {
                        continue;
                    }
                    return Err(anyhow::anyhow!(
                        "the publication changed during each metadata observation"
                    )
                    .context(airk_store::current::CurrentGateContention::Applying));
                }
                result => return result,
            }
        }
    }
    fn scoped_index_metadata_once(
        &self,
        project: airk_core::service::ProjectBinding,
        view: ScopedIndexMetadataView,
    ) -> Result<ScopedIndexMetadata> {
        let scope = self.read_scope.as_ref().ok_or(ScopeReadRefused)?;
        scope.live()?;
        ensure!(scope.authority.backing() == project, ScopeReadRefused);
        let read = self.current_reader()?;
        self.require_index_metadata(&read)?;
        let token = read.token()?;
        if !matches!(view, ScopedIndexMetadataView::Roots) {
            let config =
                airk_store::roots::RootConfig::load(self.store.directory(), &self.root_id())?;
            if let Err(error) = self.require_configured_index_metadata(&read, &config.roots) {
                // A configuration ahead of a publication that has already
                // moved on is observed again against the newer one.
                if error.is::<crate::retrieval_retry::ScopeReconcilePending>()
                    && self.current_reader()?.token()? != token
                {
                    return Err(PublicationMoved.into());
                }
                return Err(error);
            }
        }
        let value = match view {
            ScopedIndexMetadataView::Roots => serde_json::to_value(self.roots()?)?,
            // Canonical bodies keep the installed scope attached. Their status
            // projection deliberately never probes mutable source availability.
            ScopedIndexMetadataView::VectorStatus => {
                serde_json::to_value(self.index_vector_status()?)?
            }
            ScopedIndexMetadataView::VectorSelection => {
                serde_json::to_value(self.index_vector_selection_status()?)?
            }
            ScopedIndexMetadataView::ExtractionPolicy => {
                serde_json::to_value(self.rebuild_extraction_policy()?)?
            }
            ScopedIndexMetadataView::ScanLimits => {
                serde_json::to_value(self.index_scan_limits_description()?)?
            }
        };
        // The view holds its own readers; this one is not needed again.
        drop(read);
        #[cfg(test)]
        if let Some(observed) = scope.metadata_observed.lock().unwrap().as_ref() {
            observed();
        }
        scope.live()?;
        let current = self.current_reader()?;
        ensure!(current.token()? == token, PublicationMoved);
        self.require_index_metadata(&current)?;
        let publication = airk_core::evidence::RevisionEnvelope::new(
            self.collection.clone(),
            crate::current::runtime::revision(token),
        );
        if let Some(existing) = value.get("publication").filter(|value| !value.is_null()) {
            // The view's own reader observed a newer publication.
            ensure!(
                serde_json::from_value::<airk_core::identity::RevisionToken>(existing.clone())?
                    == publication.token,
                PublicationMoved
            );
        }
        let mut value = value;
        let object = value.as_object_mut().ok_or(ScopeReadRefused)?;
        object.insert("scope_observation".into(), scope.observation_value.clone());
        object.insert(
            "scope_publication".into(),
            serde_json::to_value(&publication)?,
        );
        scope.live()?;
        Ok(ScopedIndexMetadata {
            value,
            observation: scope.observation.clone(),
            publication,
        })
    }
    /// Constructs a separate immutable session for an in-memory W4/A2
    /// selection; the administrative Kernel is untouched. The owning plane's
    /// re-check and native admission remain the host's responsibility; this
    /// frozen payload grants nothing by itself.
    pub fn with_selection_reads(&self, authority: SelectionReadAuthority) -> Result<Self> {
        self.with_authority(Authority::Selection(authority))
    }
    /// The same separate immutable session for a U105 open agent read.
    /// No host installs it yet (the worker refuses the carrier).
    pub fn with_open_reads(&self, authority: OpenReadAuthority) -> Result<Self> {
        self.with_authority(Authority::Open(authority))
    }
    fn with_authority(&self, authority: Authority) -> Result<Self> {
        ensure!(self.read_scope.is_none(), ScopeReadRefused);
        let fingerprint = authority.fingerprint()?;
        let binding = airk_core::service::ProjectRegistration::new(
            self.root_id(),
            self.data_root.to_str().ok_or(ScopeReadRefused)?.into(),
            self.collection.clone(),
        )?
        .binding;
        ensure!(authority.backing() == binding, ScopeReadRefused);
        let observation = authority.observation(fingerprint)?;
        let observation_value = serde_json::to_value(&observation)?;
        let mut session = self.clone();
        session.read_scope = Some(Arc::new(SessionScope {
            authority: Arc::new(authority),
            fingerprint,
            observation,
            observation_value,
            revoked: Arc::new(AtomicBool::new(false)),
            compiled: Mutex::new(None),
            #[cfg(test)]
            reader_attachments: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            service_source_checks: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            metadata_observed: Mutex::new(None),
        }));
        Ok(session)
    }
    /// The same frozen scoped-read authority, observation and selected
    /// snapshot with a revocation flag of its own: an Answer job's reads
    /// (arbiter CONTRACT, "Answer job kernel"). The source session's
    /// revocation no longer reaches the copy; the copy's own does. Refused
    /// when the source session is already revoked. A kernel without scoped
    /// reads is returned unchanged.
    pub(crate) fn with_own_read_revocation(&self) -> Result<Self> {
        let Some(scope) = &self.read_scope else {
            return Ok(self.clone());
        };
        scope.live()?;
        let mut kernel = self.clone();
        kernel.read_scope = Some(Arc::new(SessionScope {
            authority: scope.authority.clone(),
            fingerprint: scope.fingerprint,
            observation: scope.observation.clone(),
            observation_value: scope.observation_value.clone(),
            revoked: Arc::new(AtomicBool::new(false)),
            compiled: Mutex::new(None),
            #[cfg(test)]
            reader_attachments: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            service_source_checks: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            metadata_observed: Mutex::new(None),
        }));
        Ok(kernel)
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
        airk_store::service::collection::validate_split_store_access(&self.store)?;
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
    /// Attribute an already resolved native source to its exact selected
    /// memberships. No filesystem read or virtual-alias inference occurs.
    pub(crate) fn scoped_source_attribution(
        &self,
        root: &str,
        path: &str,
    ) -> Result<
        Option<(
            IndexId,
            airk_core::index_catalog::SourceRootId,
            Vec<QueryProjectId>,
        )>,
    > {
        let Some(scope) = &self.read_scope else {
            return Ok(None);
        };
        scope.live()?;
        airk_core::index_catalog::relative_path(path, false)?;
        let source = scope
            .authority
            .sources()
            .iter()
            .find(|source| source.original_root == root)
            .ok_or(ScopeReadRefused)?;
        let projects = scope
            .authority
            .attributed_projects(source.source_root_id, path);
        // Every source of an open read is one whole Subtree "" atom, so a
        // resolved source of it is in scope with no project attribution.
        ensure!(
            !projects.is_empty() || matches!(*scope.authority, Authority::Open(_)),
            ScopeReadRefused
        );
        scope.live()?;
        Ok(Some((
            scope.authority.index_id(),
            source.source_root_id,
            projects,
        )))
    }
    /// Bounded installed metadata only. Native admission/delivery checks remain
    /// authoritative; this does not discover the machine catalog or restore grants.
    pub fn query_scope(
        &self,
        args: &airk_core::combined_reads::query_scope::QueryScopeArgs,
    ) -> Result<serde_json::Value> {
        self.scoped_read_fingerprint()?;
        let body = match self.scope_observation() {
            Some(observation) => {
                airk_core::combined_reads::query_scope::installed_body(args, observation)?
            }
            None => airk_core::combined_reads::query_scope::legacy_body(args)?,
        };
        self.scoped_read_fingerprint()?;
        Ok(body)
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
        self.annotate_scoped_read_within(value, airk_core::MAX_RESPONSE_BYTES)
    }
    /// As [`Self::annotate_scoped_read`], bounded by the caller's reply frame:
    /// Answer's 131,072 bytes, Parse's validated plan output allowance (at
    /// least 65,536, at most 524,288), or every other tool's 65,536.
    pub fn annotate_scoped_read_within(
        &self,
        value: &mut serde_json::Value,
        maximum_bytes: usize,
    ) -> Result<()> {
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
            required_bytes <= maximum_bytes,
            airk_core::scoped_reads::ScopeReplyBudget {
                required_bytes,
                maximum_bytes
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
        let scoped = kernel.with_selection_reads(authority)?;
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
        let scoped = kernel.with_selection_reads(authority)?;
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
        let full = admin.with_selection_reads(authority.clone())?;
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
        authority.atoms[0].1.path = "public".into();
        let subset = admin.with_selection_reads(authority)?;
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
        // Entry 67 addendum: the configured root is outside this primary-only
        // authority, so publishing it would leave the scope partial. That is
        // a refusal now, never a pending reconcile that could not lift.
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
    #[test]
    fn full_project_metadata_and_readiness_survive_actual_permanent_deny() -> Result<()> {
        use airk_core::source_policy::{
            DenyReason, DenyRule, POLICY_SCHEMA, SourcePolicy, SourceRoot,
        };
        let temporary = tempfile::tempdir()?;
        let source = temporary.path().join("source");
        std::fs::create_dir_all(source.join("public"))?;
        std::fs::create_dir_all(source.join("private"))?;
        let allowed = b"pub struct AllowedMetadata;\n";
        let denied = b"pub struct HiddenMetadata;\n";
        std::fs::write(source.join("public/main.rs"), allowed)?;
        std::fs::write(source.join("private/hidden.rs"), denied)?;
        let admin = Kernel::open_service_collection(
            &source,
            Some(temporary.path().join("data")),
            "policy-metadata",
        )?;
        admin.configure_vectors(false)?;
        admin.index()?;
        let registration = airk_core::service::ProjectRegistration::new(
            admin.root_id(),
            admin.data_root.to_str().unwrap().into(),
            admin.collection.clone(),
        )?;
        let mut authority = fixture(&admin.root_id());
        authority.backing = registration.binding;
        let source_id = authority.sources[0].source_root_id;
        let policy = SourcePolicy {
            schema: POLICY_SCHEMA.into(),
            index_id: authority.index_id,
            owner_binding: registration.binding,
            namespace: "collections-1.4".into(),
            collection: admin.collection.clone(),
            epoch: CatalogRevision::INITIAL,
            roots: vec![SourceRoot {
                id: source_id,
                canonical_path: registration.root,
                aliases: vec![],
                owner: true,
            }],
            positive_includes: None,
            denies: vec![DenyRule {
                source_root_id: source_id,
                kind: PathScope::Subtree,
                path: "private".into(),
                rule_id: DenyId::from_identity(Incarnation::from_bytes([91; 16])),
                reason: DenyReason::Trim,
                created_epoch: CatalogRevision::INITIAL,
                inherited_from: None,
            }],
        };
        policy.validate()?;
        let bound = admin.with_expected_source_policy(Some(policy.clone()))?;
        let application = bound.apply_source_policy(&policy, Incarnation::from_bytes([92; 16]))?;
        assert_eq!(application.removed_artifacts, 1);
        assert!(application.removal_publication.is_some());
        authority.policy_epoch = policy.epoch.get();
        authority.policy_digest = Some(policy.identity()?);
        let full = bound.with_selection_reads(authority.clone())?;
        let read = full.current_reader()?;
        assert!(
            read.restricts_paths(),
            "permanent deny must keep Store filtering active"
        );
        assert!(!read.selected_scope_restricts_paths());
        assert!(
            read.require_path("private/hidden.rs")
                .unwrap_err()
                .is::<ScopeReadRefused>()
        );
        let token = crate::current::runtime::revision(read.token()?);
        drop(read);
        let summary = full.status_summary()?;
        assert_eq!(summary["files"], 1);
        assert_eq!(
            summary["source_policy"]["control_dependencies"],
            serde_json::to_value(airk_ingest::permanent_policy::control_dependencies())?
        );
        assert_eq!(full.status()?.files, 1);
        assert!(full.roots()?.indexed.is_empty());
        assert!(
            full.scoped_index_metadata(
                registration.binding,
                ScopedIndexMetadataView::ExtractionPolicy
            )
            .is_ok()
        );
        assert!(full.readiness_observation(token)?.is_some());
        let query = airk_core::SearchQuery {
            text: "AllowedMetadata".into(),
            path_prefix: None,
            limit: 10,
            max_bytes: 4096,
        };
        let result = full.search(&query)?;
        assert!(result.semantic_readiness.is_some() && result.discovery_readiness.is_some());
        assert!(
            !full
                .list_files("", 16, 8192)?
                .paths
                .iter()
                .any(|path| path.starts_with("private/"))
        );
        let mut subset_authority = authority.clone();
        subset_authority.atoms[0].1.path = "public".into();
        let subset = bound.with_selection_reads(subset_authority)?;
        assert!(subset.current_reader()?.selected_scope_restricts_paths());
        assert!(
            subset
                .status_summary()
                .unwrap_err()
                .is::<ScopeReadRefused>()
        );
        assert!(subset.roots().unwrap_err().is::<ScopeReadRefused>());
        let result = subset.search(&query)?;
        assert!(result.semantic_readiness.is_none() && result.discovery_readiness.is_none());
        assert!(subset.readiness_observation(token)?.is_none());
        let mut stale = authority;
        stale.policy_epoch = 0;
        stale.policy_digest = None;
        assert!(
            bound
                .with_selection_reads(stale)?
                .status_summary()
                .unwrap_err()
                .is::<ScopeReadRefused>()
        );
        full.revoke_scoped_reads();
        assert!(full.status_summary().unwrap_err().is::<ScopeReadRefused>());
        assert_eq!(std::fs::read(source.join("public/main.rs"))?, allowed);
        assert_eq!(std::fs::read(source.join("private/hidden.rs"))?, denied);
        Ok(())
    }

    #[test]
    fn source_projection_uses_exact_retained_aliases_and_only_selected_sources() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let primary = directory.path().join("primary");
        let external = directory.path().join("external");
        let private = directory.path().join("private");
        for root in [&primary, &external, &private] {
            std::fs::create_dir(root)?;
        }
        std::fs::write(primary.join("main.rs"), "pub struct Primary;\n")?;
        std::fs::write(external.join("allowed.rs"), "pub struct External;\n")?;
        std::fs::write(private.join("private.rs"), "pub struct Private;\n")?;
        let admin = Kernel::open(&primary, Some(directory.path().join("data")))?;
        admin.configure_vectors(false)?;
        admin.add_root("old", &external)?;
        admin.add_root("hidden", &private)?;
        admin.index()?;
        let original_roots = admin.configured_roots()?;
        let mut authority = fixture(&admin.root_id());
        authority.backing = airk_core::service::ProjectRegistration::new(
            admin.root_id(),
            admin.data_root.to_string_lossy().into_owned(),
            admin.collection.clone(),
        )?
        .binding;
        let backing = authority.backing;
        let source = SourceRootId::from_identity(Incarnation::from_bytes([9; 16]));
        authority.sources.push(SourceDescriptor {
            source_root_id: source,
            original_root: original_roots["old"].clone(),
        });
        authority.atoms.push((
            authority.project_ids[0],
            ScopeAtom {
                source_root_id: source,
                kind: PathScope::Exact,
                path: "allowed.rs".into(),
            },
        ));
        let scoped = admin.with_selection_reads(authority)?;
        let token = crate::current::runtime::revision(admin.current_reader()?.token()?);
        let initial = scoped.scoped_source_projection(backing, token)?;
        assert_eq!(initial.sources.len(), 2);
        assert_eq!(
            initial
                .sources
                .iter()
                .find(|entry| entry.source_root_id == source)
                .unwrap()
                .aliases,
            vec!["old"]
        );
        assert!(
            initial
                .sources
                .iter()
                .all(|entry| entry.original_root != original_roots["hidden"])
        );
        admin.remove_root("old")?;
        admin.add_root("new", &external)?;
        // Removing the isolated fixture's live external bytes demonstrates that
        // this proof uses retained metadata, not live location or hash probes.
        std::fs::remove_file(external.join("allowed.rs"))?;
        std::fs::remove_dir(&external)?;
        let pending = scoped.scoped_source_projection(backing, token)?;
        assert_eq!(
            pending, initial,
            "current aliases must never rename a retained citation origin"
        );
        assert!(
            pending
                .sources
                .iter()
                .all(|entry| !entry.aliases.contains(&"new".into()))
        );
        let mut wrong = token;
        wrong.generation += 1;
        assert!(scoped.scoped_source_projection(backing, wrong).is_err());
        assert!(
            scoped
                .scoped_source_projection(Sha256Digest::from_bytes([0; 32]), token)
                .is_err()
        );
        assert!(admin.scoped_source_projection(backing, token).is_err());
        scoped.revoke_scoped_reads();
        assert!(scoped.scoped_source_projection(backing, token).is_err());
        assert_eq!(
            std::fs::read_to_string(primary.join("main.rs"))?,
            "pub struct Primary;\n"
        );
        Ok(())
    }
    /// A window selection of one project over the whole of one source.
    fn fixture(root: &str) -> SelectionReadAuthority {
        let project = QueryProjectId::from_identity(Incarnation::from_bytes([1; 16]));
        let index = IndexId::from_identity(Incarnation::from_bytes([2; 16]));
        let source = SourceRootId::from_identity(Incarnation::from_bytes([3; 16]));
        SelectionReadAuthority {
            schema: 1,
            backing: Sha256Digest::from_bytes([6; 32]),
            index_id: index,
            project_ids: vec![project],
            selection: SelectionScopeRef {
                owner: airk_core::scoped_reads::SelectionOwnerKind::Window,
                grant_nonce: Incarnation::from_bytes([10; 16]),
                service_instance: Incarnation::from_bytes([11; 16]),
                scope_identity: Sha256Digest::from_bytes([12; 32]),
                catalog_revision: CatalogRevision::try_from(5_u64).unwrap(),
            },
            atoms: vec![(
                project,
                ScopeAtom {
                    source_root_id: source,
                    kind: PathScope::Subtree,
                    path: String::new(),
                },
            )],
            sources: vec![SourceDescriptor {
                source_root_id: source,
                original_root: root.into(),
            }],
            membership_revisions: vec![(project, CatalogRevision::INITIAL)],
            source_revision: CatalogRevision::INITIAL,
            policy_epoch: 0,
            policy_digest: None,
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
        authority.atoms[0].1.path = "@external/private.rs".into();
        assert!(authority.compile(original, &published).is_err());
        authority.atoms[0].1.path.clear();
        let aliases = BTreeMap::from([("docs".into(), original.into())]);
        let aliased = authority.compile("different-primary", &aliases)?;
        assert!(aliased.allows_path("@docs/lib.rs"));
        assert!(!aliased.allows_path("lib.rs"));
        assert!(
            authority
                .compile("different-primary", &BTreeMap::new())
                .is_err()
        );
        authority.atoms[0].1.kind = PathScope::Exact;
        authority.atoms[0].1.path = "one.rs".into();
        let exact = authority.compile(original, &BTreeMap::new())?;
        assert!(exact.allows_path("one.rs"));
        assert!(!exact.allows_path("one.rs/child"));
        let before = authority.fingerprint()?;
        authority.selection.catalog_revision = CatalogRevision::try_from(6_u64)?;
        assert_ne!(before, authority.fingerprint()?);
        let before = authority.fingerprint()?;
        authority.membership_revisions[0].1 = CatalogRevision::INITIAL.next()?;
        assert_ne!(before, authority.fingerprint()?);
        let before = authority.fingerprint()?;
        authority.source_revision = CatalogRevision::INITIAL.next()?;
        assert_ne!(before, authority.fingerprint()?);
        let before = authority.fingerprint()?;
        authority.policy_epoch = 1;
        authority.policy_digest = Some(Sha256Digest::from_bytes([7; 32]));
        assert_ne!(before, authority.fingerprint()?);
        // A read project without atoms is never a selection read.
        authority.atoms.clear();
        authority.sources.clear();
        assert!(authority.compile(original, &BTreeMap::new()).is_err());
        Ok(())
    }
    #[test]
    fn authority_has_one_bounded_frame() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut authority = fixture(dir.path().to_str().unwrap());
        let source = authority.sources[0].source_root_id;
        let project = authority.project_ids[0];
        authority.atoms = (0..32)
            .map(|index| {
                (
                    project,
                    ScopeAtom {
                        source_root_id: source,
                        kind: PathScope::Exact,
                        path: format!("{index}/{}", "x".repeat(2000)),
                    },
                )
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
        assert!(admin.with_selection_reads(authority.clone()).is_err());
        authority.backing = airk_core::service::ProjectRegistration::new(
            admin.root_id(),
            admin.data_root.to_str().unwrap().into(),
            admin.collection.clone(),
        )?
        .binding;
        let session = admin.with_selection_reads(authority)?;
        assert!(admin.require_unrestricted_read().is_ok());
        assert!(
            session
                .require_unrestricted_read()
                .unwrap_err()
                .is::<ScopeReadRefused>()
        );
        assert!(
            session
                .with_selection_reads(fixture(root.to_str().unwrap()))
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
    fn selection_fixture(root: &str) -> SelectionReadAuthority {
        fixture(root)
    }
    #[test]
    fn selection_authority_validation_mirrors_the_catalog_rules() -> Result<()> {
        type Change = Box<dyn Fn(&mut SelectionReadAuthority)>;
        fn change(edit: impl Fn(&mut SelectionReadAuthority) + 'static) -> Change {
            Box::new(edit)
        }
        let dir = tempfile::tempdir()?;
        let valid = selection_fixture(dir.path().to_str().unwrap());
        valid.validate()?;
        let project = valid.project_ids[0];
        let other = QueryProjectId::from_identity(Incarnation::from_bytes([20; 16]));
        let unused = SourceRootId::from_identity(Incarnation::from_bytes([21; 16]));
        let six = CatalogRevision::try_from(6_u64)?;
        let cases: Vec<(&str, Change)> = vec![
            ("schema", change(|a| a.schema = 2)),
            (
                "no project",
                change(|a| {
                    a.project_ids.clear();
                    a.atoms.clear();
                    a.membership_revisions.clear();
                    a.sources.clear();
                }),
            ),
            (
                "duplicate project",
                change(move |a| a.project_ids.push(project)),
            ),
            (
                "project without atoms",
                change(move |a| {
                    a.project_ids.push(other);
                    a.membership_revisions
                        .push((other, CatalogRevision::INITIAL));
                }),
            ),
            (
                "atom outside the read projects",
                change(move |a| {
                    let atom = a.atoms[0].1.clone();
                    a.atoms.push((other, atom));
                }),
            ),
            (
                "missing membership revision",
                change(|a| a.membership_revisions.clear()),
            ),
            (
                "duplicate membership revision",
                change(|a| {
                    let revision = a.membership_revisions[0];
                    a.membership_revisions.push(revision);
                }),
            ),
            (
                "extra membership revision",
                change(move |a| {
                    a.membership_revisions
                        .push((other, CatalogRevision::INITIAL))
                }),
            ),
            (
                "source outside the atom union",
                change(move |a| {
                    let original_root = format!("{}-unused", a.sources[0].original_root);
                    a.sources.push(SourceDescriptor {
                        source_root_id: unused,
                        original_root,
                    });
                }),
            ),
            ("atom source not described", change(|a| a.sources.clear())),
            (
                "duplicate atom",
                change(|a| {
                    let atom = a.atoms[0].clone();
                    a.atoms.push(atom);
                }),
            ),
            (
                "invalid atom path",
                change(|a| a.atoms[0].1.path = "../outside".into()),
            ),
            (
                "relative source root",
                change(|a| a.sources[0].original_root = "relative".into()),
            ),
            (
                "policy digest without epoch",
                change(|a| a.policy_digest = Some(Sha256Digest::from_bytes([7; 32]))),
            ),
            (
                "policy epoch without digest",
                change(|a| a.policy_epoch = 1),
            ),
            (
                "source newer than selection",
                change(move |a| a.source_revision = six),
            ),
            (
                "membership newer than selection",
                change(move |a| a.membership_revisions[0].1 = six),
            ),
            (
                "more than MAX_SELECTION projects",
                change(|a| {
                    let atom = a.atoms[0].1.clone();
                    for n in 0..MAX_SELECTION as u8 {
                        let id =
                            QueryProjectId::from_identity(Incarnation::from_bytes([100 + n; 16]));
                        a.project_ids.push(id);
                        a.atoms.push((id, atom.clone()));
                        a.membership_revisions.push((id, CatalogRevision::INITIAL));
                    }
                }),
            ),
            (
                "over 48 KiB",
                change(|a| {
                    let (project, source) = (a.project_ids[0], a.atoms[0].1.source_root_id);
                    a.atoms = (0..32)
                        .map(|index| {
                            (
                                project,
                                ScopeAtom {
                                    source_root_id: source,
                                    kind: PathScope::Exact,
                                    path: format!("{index}/{}", "x".repeat(2000)),
                                },
                            )
                        })
                        .collect();
                }),
            ),
        ];
        for (name, change) in cases {
            let mut refused = valid.clone();
            change(&mut refused);
            assert!(refused.validate().is_err(), "{name}");
            assert!(refused.fingerprint().is_err(), "{name}");
        }
        let mut two = valid.clone();
        let atom = two.atoms[0].1.clone();
        two.project_ids.push(other);
        two.atoms.push((other, atom));
        two.membership_revisions
            .push((other, CatalogRevision::INITIAL));
        two.validate()?;
        Ok(())
    }
    #[test]
    fn selection_fingerprint_uses_its_own_reviewed_domain() -> Result<()> {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/split/i1/selection_authority.json"
        ))?;
        let case = &fixture["selection_fingerprint"];
        assert_eq!(case["domain"], "airk/scoped-read-authority/selection/1");
        let bytes = case["authority"].as_str().unwrap();
        let authority: SelectionReadAuthority = serde_json::from_str(bytes)?;
        assert_eq!(serde_json::to_string(&authority)?, bytes);
        let expected: Sha256Digest = case["fingerprint"].as_str().unwrap().parse()?;
        assert_eq!(authority.digest()?, expected);
        // The removed catalog-grant domain never collides with it.
        let catalog_domain: Sha256Digest =
            case["catalog_domain_digest"].as_str().unwrap().parse()?;
        assert_eq!(
            framed_digest(b"airk/scoped-read-authority/1", &[bytes.as_bytes()]),
            catalog_domain
        );
        assert_ne!(expected, catalog_domain);
        let dir = tempfile::tempdir()?;
        let live = selection_fixture(dir.path().to_str().unwrap());
        assert_eq!(live.fingerprint()?, live.digest()?);
        Ok(())
    }
    /// A selection and an open read over the same whole-source atoms compile
    /// to the same predicates; only the fingerprint differs.
    #[test]
    fn selection_compile_equals_open_compile_for_the_same_atoms() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let original = dir.path().to_str().unwrap();
        let selection = fixture(original);
        let open = OpenReadAuthority {
            schema: 1,
            backing: selection.backing,
            index_id: selection.index_id,
            selection: SelectionScopeRef {
                owner: airk_core::scoped_reads::SelectionOwnerKind::Agent,
                ..selection.selection
            },
            sources: selection.sources.clone(),
            source_revision: selection.source_revision,
            policy_epoch: selection.policy_epoch,
            policy_digest: selection.policy_digest,
        };
        let aliased = BTreeMap::from([("docs".to_string(), original.to_string())]);
        for (primary, aliases) in [(original, BTreeMap::new()), ("different-primary", aliased)] {
            let from_open = open.compile(primary, &aliases)?;
            let from_selection = selection.compile(primary, &aliases)?;
            assert_eq!(from_selection.atoms, from_open.atoms);
            assert_eq!(from_selection.source_atoms, from_open.source_atoms);
            assert_eq!(from_selection.external_aliases, from_open.external_aliases);
            assert_eq!(from_open.fingerprint, open.fingerprint()?);
            assert_eq!(from_selection.fingerprint, selection.fingerprint()?);
            assert_ne!(from_selection.fingerprint, from_open.fingerprint);
        }
        assert!(
            selection
                .compile("different-primary", &BTreeMap::new())
                .is_err()
        );
        Ok(())
    }
    #[test]
    fn selection_observation_names_the_selection_and_never_the_nonce() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().to_str().unwrap();
        let authority = selection_fixture(root);
        let observation = authority.observation(authority.fingerprint()?)?;
        assert_eq!(observation.selection, authority.selection.observation());
        assert_eq!(observation.members.len(), 1);
        assert_eq!(observation.members[0].index_id, authority.index_id);
        assert_eq!(
            observation.members[0].memberships[0].membership_revision,
            authority.membership_revisions[0].1
        );
        let value = serde_json::to_value(&observation)?;
        assert_eq!(
            value["selection"],
            serde_json::json!({"owner":"window","scope_identity":"0c".repeat(32),"catalog_revision":5})
        );
        assert!(value.get("grant").is_none() && value.get("provenance").is_none());
        let text = value.to_string();
        assert!(
            !text.contains(&"0a".repeat(16)),
            "the nonce never leaves the service"
        );
        assert!(!text.contains(&"0b".repeat(16)));
        Ok(())
    }
    #[test]
    fn selection_session_is_separate_annotated_and_revocable() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("source");
        std::fs::create_dir(&root)?;
        let admin = Kernel::open(&root, Some(dir.path().join("data")))?;
        let mut authority = selection_fixture(&admin.root_id());
        assert!(admin.with_selection_reads(authority.clone()).is_err());
        authority.backing = airk_core::service::ProjectRegistration::new(
            admin.root_id(),
            admin.data_root.to_str().unwrap().into(),
            admin.collection.clone(),
        )?
        .binding;
        let session = admin.with_selection_reads(authority.clone())?;
        assert_eq!(
            session.scoped_read_fingerprint()?,
            Some(authority.fingerprint()?)
        );
        let mut value = serde_json::json!({"paths":[]});
        session.annotate_scoped_read(&mut value)?;
        assert_eq!(
            value["scope_observation"],
            serde_json::to_value(session.scope_observation())?
        );
        assert_eq!(value["scope_observation"]["selection"]["owner"], "window");
        assert!(value["scope_observation"].get("grant").is_none());
        assert_eq!(
            session.scoped_source_attribution(&admin.root_id(), "lib.rs")?,
            Some((
                authority.index_id,
                authority.sources[0].source_root_id,
                authority.project_ids.clone()
            ))
        );
        session.require_scoped_policy_binding(None)?;
        assert!(
            session
                .require_unrestricted_read()
                .unwrap_err()
                .is::<ScopeReadRefused>()
        );
        assert!(session.with_selection_reads(authority).is_err());
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
    fn open_from(selection: &SelectionReadAuthority) -> OpenReadAuthority {
        let mut scope = selection.selection;
        scope.owner = airk_core::scoped_reads::SelectionOwnerKind::Agent;
        OpenReadAuthority {
            schema: 1,
            backing: selection.backing,
            index_id: selection.index_id,
            selection: scope,
            sources: selection.sources.clone(),
            source_revision: selection.source_revision,
            policy_epoch: selection.policy_epoch,
            policy_digest: selection.policy_digest,
        }
    }
    /// U106(d): the open authority's reviewed bytes and its own domain.
    #[test]
    fn open_fingerprint_uses_its_own_reviewed_domain() -> Result<()> {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/split/agents/open_access.json"
        ))?;
        let case = &fixture["open_authority"];
        assert_eq!(case["domain"], "airk/scoped-read-authority/open/1");
        let bytes = case["authority"].as_str().unwrap();
        let authority: OpenReadAuthority = serde_json::from_str(bytes)?;
        assert_eq!(serde_json::to_string(&authority)?, bytes);
        let expected: Sha256Digest = case["fingerprint"].as_str().unwrap().parse()?;
        assert_eq!(authority.digest()?, expected);
        let selection_domain: Sha256Digest =
            case["selection_domain_digest"].as_str().unwrap().parse()?;
        assert_eq!(
            framed_digest(
                b"airk/scoped-read-authority/selection/1",
                &[bytes.as_bytes()]
            ),
            selection_domain
        );
        assert_ne!(expected, selection_domain);
        Ok(())
    }
    #[test]
    fn open_authority_reads_every_source_whole_and_refuses_other_owners() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("source");
        std::fs::create_dir(&root)?;
        let admin = Kernel::open(&root, Some(dir.path().join("data")))?;
        let mut authority = open_from(&selection_fixture(&admin.root_id()));
        authority.backing = airk_core::service::ProjectRegistration::new(
            admin.root_id(),
            admin.data_root.to_str().unwrap().into(),
            admin.collection.clone(),
        )?
        .binding;
        authority.validate()?;
        type Change = fn(&mut OpenReadAuthority);
        let cases: [(&str, Change); 5] = [
            ("schema", |a| a.schema = 2),
            ("window owner", |a| {
                a.selection.owner = airk_core::scoped_reads::SelectionOwnerKind::Window
            }),
            ("no source", |a| a.sources.clear()),
            ("duplicate source", |a| a.sources.push(a.sources[0].clone())),
            ("policy digest without epoch", |a| {
                a.policy_digest = Some(Sha256Digest::from_bytes([1; 32]))
            }),
        ];
        for (name, change) in cases {
            let mut refused = authority.clone();
            change(&mut refused);
            assert!(refused.fingerprint().is_err(), "{name}");
            assert!(admin.with_open_reads(refused).is_err(), "{name}");
        }
        let compiled = authority.compile(&admin.root_id(), &BTreeMap::new())?;
        assert_eq!(compiled.atoms.len(), 1);
        assert!(compiled.atoms[0].path.is_empty());
        let session = admin.with_open_reads(authority.clone())?;
        assert_eq!(
            session.scoped_read_fingerprint()?,
            Some(authority.fingerprint()?)
        );
        let observation = serde_json::to_value(session.scope_observation())?;
        assert_eq!(observation["selection"]["owner"], "agent");
        assert_eq!(
            observation["members"][0]["memberships"],
            serde_json::json!([])
        );
        // A whole-index read attributes the source to the index, no project.
        assert_eq!(
            session.scoped_source_attribution(&admin.root_id(), "lib.rs")?,
            Some((
                authority.index_id,
                authority.sources[0].source_root_id,
                Vec::new()
            ))
        );
        // A root outside the authority's sources is still refused.
        assert!(
            session
                .scoped_source_attribution("/not/a/source", "lib.rs")
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn authenticated_genesis_waits_for_configuration_without_weakening_scope() -> Result<()> {
        use airk_core::{
            graph_storage::{GraphHead, GraphScope},
            scoped_reads::{SelectionOwnerKind, SelectionScopeRef},
            service_resources::WaitReason,
        };
        use airk_store::current::{CurrentHead, CurrentScope, collection::CollectionConfiguration};

        let directory = tempfile::tempdir()?;
        let source = directory.path().join("source");
        std::fs::create_dir(&source)?;
        let admin = Kernel::open(&source, Some(directory.path().join("data")))?;
        admin.configure_vectors(false)?;
        let scope = CurrentScope {
            root: admin.root_id(),
            collection: admin.collection.clone(),
            incarnation: Incarnation::from_bytes([11; 16]),
        };
        let lexical = airk_store::current::lexical::LexicalManifest::empty()?;
        let head = CurrentHead {
            version: 2,
            scope: scope.clone(),
            sequence: 0,
            artifact_root: airk_store::current::map::empty_root(&scope)?,
            lexical_sequence: 0,
            lexical_commit: lexical.digest()?,
            graph_head: GraphHead::empty(GraphScope {
                root: scope.root.clone(),
                collection: scope.collection.clone(),
                incarnation: scope.incarnation,
                graph_instance: Incarnation::from_bytes([12; 16]),
                specification: Sha256Digest::from_bytes([13; 32]),
                record_bytes: 16,
            })?,
        };
        let quota = admin.collection_quota()?;
        {
            let lock = admin.store.lock()?;
            let mut current = admin.store.current_write(&lock, &quota, &scope, 16 << 20)?;
            current.initialize(head.clone())?;
            current
                .install_initial_lexical_manifest(&head.token()?, &lexical.canonical_bytes()?)?;
        }
        {
            let raw = admin.store.current_read(&scope)?;
            assert_eq!(raw.head().sequence, 0);
            assert!(raw.collection_state()?.is_none());
        }
        let template = fixture(&admin.root_id());
        let authority = OpenReadAuthority {
            schema: 1,
            backing: airk_core::service::ProjectRegistration::new(
                admin.root_id(),
                admin.data_root.to_str().unwrap().into(),
                admin.collection.clone(),
            )?
            .binding,
            index_id: template.index_id,
            selection: SelectionScopeRef {
                owner: SelectionOwnerKind::Agent,
                grant_nonce: Incarnation::from_bytes([14; 16]),
                service_instance: Incarnation::from_bytes([15; 16]),
                scope_identity: Sha256Digest::from_bytes([16; 32]),
                catalog_revision: CatalogRevision::INITIAL,
            },
            sources: template.sources,
            source_revision: CatalogRevision::INITIAL,
            policy_epoch: 0,
            policy_digest: None,
        };
        let selected = admin.with_open_reads(authority.clone())?;
        for kernel in [&admin, &selected] {
            let error = kernel.status_summary().unwrap_err();
            assert!(!error.is::<ScopeReadRefused>());
            let hint =
                crate::retrieval_retry::read_retry_hint(&error).expect("typed bootstrap admission");
            assert_eq!(
                serde_json::to_value(hint)?,
                serde_json::json!({
                    "status":"retry", "retryable":true, "code":"publication_busy",
                    "retry_after_ms":25, "action":"repeat_same_request"
                })
            );
            assert_eq!(
                Kernel::service_enrichment_wait_reason(&error),
                Some(WaitReason::Publication)
            );
        }
        let revoked = admin.with_open_reads(authority.clone())?;
        revoked.revoke_scoped_reads();
        let mut foreign_root = authority.clone();
        foreign_root.sources[0].original_root =
            directory.path().join("foreign").to_str().unwrap().into();
        let foreign_root = admin.with_open_reads(foreign_root)?;
        let mut wrong_policy = authority.clone();
        wrong_policy.policy_epoch = 1;
        wrong_policy.policy_digest = Some(Sha256Digest::from_bytes([17; 32]));
        let wrong_policy = admin.with_open_reads(wrong_policy)?;
        for kernel in [&revoked, &foreign_root, &wrong_policy] {
            let error = kernel.status_summary().unwrap_err();
            assert!(error.is::<ScopeReadRefused>());
            assert!(crate::retrieval_retry::read_retry_hint(&error).is_none());
        }
        let mut foreign_backing = authority;
        foreign_backing.backing = Sha256Digest::from_bytes([18; 32]);
        assert!(
            admin
                .with_open_reads(foreign_backing)
                .err()
                .unwrap()
                .is::<ScopeReadRefused>()
        );

        let policy = admin.rebuild_extraction_policy()?.policy;
        let configuration = CollectionConfiguration {
            retained_profile: policy.retained_profile,
            source_policy_digest: None,
            roots: BTreeMap::new(),
            extractor_version: airk_core::EXTRACTOR_VERSION.into(),
            extraction_policy_digest: crate::extraction::policy_digest(&policy)?,
            index_detail: policy.legacy_detail(),
            vector_selection: admin.vector_selection()?,
            model: None,
        };
        {
            let lock = admin.store.lock()?;
            let mut current = admin.store.current_write(&lock, &quota, &scope, 16 << 20)?;
            current.initialize_collection(&head.token()?, &configuration)?;
        }
        let status = selected.status_summary()?;
        assert_eq!(status["index_state"], "available");
        assert_eq!(status["files"], 0);
        assert_eq!(status["generation"], 1);
        assert!(status["publication"]["token"].is_string());
        assert_eq!(selected.current_reader()?.head().sequence, 1);
        let mut partial = fixture(&admin.root_id());
        partial.backing = airk_core::service::ProjectRegistration::new(
            admin.root_id(),
            admin.data_root.to_str().unwrap().into(),
            admin.collection.clone(),
        )?
        .binding;
        partial.atoms[0].1.path = "partial".into();
        let error = admin
            .with_selection_reads(partial)?
            .status_summary()
            .unwrap_err();
        assert!(error.is::<ScopeReadRefused>());
        assert!(crate::retrieval_retry::read_retry_hint(&error).is_none());
        assert!(std::fs::read_dir(source)?.next().is_none());
        Ok(())
    }

    /// Change-log entry 67 fixtures: a primary root and an external root,
    /// indexed with no external root configured.
    struct TwoRoots {
        _directory: tempfile::TempDir,
        admin: Kernel,
        external: std::path::PathBuf,
    }
    fn two_roots() -> Result<TwoRoots> {
        let directory = tempfile::tempdir()?;
        let primary = directory.path().join("primary");
        let external = directory.path().join("external");
        for root in [&primary, &external] {
            std::fs::create_dir(root)?;
        }
        std::fs::write(primary.join("main.rs"), "pub struct Primary;\n")?;
        std::fs::write(external.join("allowed.rs"), "pub struct External;\n")?;
        let admin = Kernel::open(&primary, Some(directory.path().join("data")))?;
        admin.configure_vectors(false)?;
        Ok(TwoRoots {
            _directory: directory,
            admin,
            external,
        })
    }
    /// The whole primary plus `allowed.rs` of the root at `original_root`.
    fn with_external(admin: &Kernel, original_root: String) -> Result<SelectionReadAuthority> {
        let mut authority = fixture(&admin.root_id());
        authority.backing = airk_core::service::ProjectRegistration::new(
            admin.root_id(),
            admin.data_root.to_string_lossy().into_owned(),
            admin.collection.clone(),
        )?
        .binding;
        let source = SourceRootId::from_identity(Incarnation::from_bytes([9; 16]));
        authority.sources.push(SourceDescriptor {
            source_root_id: source,
            original_root,
        });
        authority.atoms.push((
            authority.project_ids[0],
            ScopeAtom {
                source_root_id: source,
                kind: PathScope::Exact,
                path: "allowed.rs".into(),
            },
        ));
        Ok(authority)
    }
    /// The whole primary and the whole root at `original_root`.
    fn with_whole_external(
        admin: &Kernel,
        original_root: String,
    ) -> Result<SelectionReadAuthority> {
        let mut authority = with_external(admin, original_root)?;
        let (_, atom) = authority.atoms.last_mut().expect("the external atom");
        atom.kind = PathScope::Subtree;
        atom.path = String::new();
        Ok(authority)
    }
    fn hint_code(error: &anyhow::Error) -> Option<&'static str> {
        crate::retrieval_retry::read_retry_hint(error).map(|hint| hint.code)
    }

    /// A root configured after the last publication: the authority that names
    /// it waits for the reconciling publication instead of being refused,
    /// and a root configured nowhere is still refused.
    #[test]
    fn configuration_ahead_of_the_publication_is_pending_reconcile_not_refusal() -> Result<()> {
        let roots = two_roots()?;
        let admin = &roots.admin;
        admin.index()?;
        admin.add_root("docs", &roots.external)?;
        let configured = admin.configured_roots()?["docs"].clone();
        let scoped = admin.with_selection_reads(with_external(admin, configured)?)?;
        let error = scoped.status_summary().unwrap_err();
        assert!(!error.is::<ScopeReadRefused>(), "{error:#}");
        assert_eq!(hint_code(&error), Some("scope_reconcile_pending"));
        let hint = crate::retrieval_retry::read_retry_hint(&error).unwrap();
        assert!(hint.retryable && hint.action == "repeat_same_request");
        let foreign = admin.with_selection_reads(with_external(
            admin,
            roots
                ._directory
                .path()
                .join("foreign")
                .to_string_lossy()
                .into_owned(),
        )?)?;
        let error = foreign.status_summary().unwrap_err();
        assert!(error.is::<ScopeReadRefused>(), "{error:#}");
        assert!(hint_code(&error).is_none());
        // The reconciling publication admits the same authority's reader.
        admin.index()?;
        scoped.current_reader()?;
        Ok(())
    }

    /// Metadata that needs the configuration to agree with the publication
    /// waits for the reconcile only when the authority covers the configured
    /// roots in full, so the same request passes once it is published
    /// (entry 67 addendum).
    #[test]
    fn scoped_metadata_with_configuration_ahead_is_pending_reconcile() -> Result<()> {
        let roots = two_roots()?;
        let admin = &roots.admin;
        admin.index()?;
        admin.add_root("docs", &roots.external)?;
        let configured = admin.configured_roots()?["docs"].clone();
        let whole = admin.with_selection_reads(with_whole_external(admin, configured)?)?;
        let backing = whole.read_scope.as_ref().unwrap().authority.backing();
        for view in [
            ScopedIndexMetadataView::VectorStatus,
            ScopedIndexMetadataView::Roots,
        ] {
            let error = whole.scoped_index_metadata(backing, view).err().unwrap();
            assert!(!error.is::<ScopeReadRefused>(), "{error:#}");
            assert_eq!(hint_code(&error), Some("scope_reconcile_pending"));
        }
        // The reconciling publication answers the same request.
        admin.index()?;
        whole.scoped_index_metadata(backing, ScopedIndexMetadataView::VectorStatus)?;
        Ok(())
    }

    /// A configured root outside a primary-only authority would leave it a
    /// partial scope once published: refused before and after, never pending.
    #[test]
    fn scoped_metadata_with_a_configured_root_outside_the_authority_refuses() -> Result<()> {
        let roots = two_roots()?;
        let admin = &roots.admin;
        admin.index()?;
        let mut authority = fixture(&admin.root_id());
        authority.backing = airk_core::service::ProjectRegistration::new(
            admin.root_id(),
            admin.data_root.to_string_lossy().into_owned(),
            admin.collection.clone(),
        )?
        .binding;
        let backing = authority.backing;
        let scoped = admin.with_selection_reads(authority)?;
        scoped.scoped_index_metadata(backing, ScopedIndexMetadataView::VectorStatus)?;
        admin.add_root("docs", &roots.external)?;
        for published in [false, true] {
            if published {
                admin.index()?;
            }
            let error = scoped
                .scoped_index_metadata(backing, ScopedIndexMetadataView::VectorStatus)
                .err()
                .unwrap();
            assert!(error.is::<ScopeReadRefused>(), "{error:#}");
            assert!(hint_code(&error).is_none());
        }
        Ok(())
    }

    /// A publication that lands while scoped metadata is read is read again;
    /// one that keeps landing is the retryable `publication_busy`.
    #[test]
    fn scoped_metadata_reads_again_when_a_publication_lands() -> Result<()> {
        for (landings, settles) in [(1usize, true), (usize::MAX, false)] {
            let roots = two_roots()?;
            let admin = roots.admin.clone();
            admin.index()?;
            let mut authority = fixture(&admin.root_id());
            authority.backing = airk_core::service::ProjectRegistration::new(
                admin.root_id(),
                admin.data_root.to_string_lossy().into_owned(),
                admin.collection.clone(),
            )?
            .binding;
            let backing = authority.backing;
            let scoped = admin.with_selection_reads(authority)?;
            let observed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let counter = observed.clone();
            let primary = roots._directory.path().join("primary");
            let data = roots._directory.path().join("data");
            *scoped
                .read_scope
                .as_ref()
                .unwrap()
                .metadata_observed
                .lock()
                .unwrap() = Some(Box::new(move || {
                let seen = counter.fetch_add(1, Ordering::SeqCst);
                if seen < landings {
                    std::fs::write(primary.join("main.rs"), format!("pub struct P{seen};\n"))
                        .unwrap();
                    // Another owner publishes, as an Index job would.
                    Kernel::open(&primary, Some(data.clone()))
                        .unwrap()
                        .index()
                        .unwrap();
                }
            }));
            let result = scoped.scoped_index_metadata(backing, ScopedIndexMetadataView::Roots);
            if settles {
                let metadata = result?;
                assert_eq!(observed.load(Ordering::SeqCst), 2);
                assert_eq!(
                    metadata.publication.token,
                    crate::current::runtime::revision(roots.admin.current_reader()?.token()?)
                );
            } else {
                let error = result.err().unwrap();
                assert!(!error.is::<ScopeReadRefused>(), "{error:#}");
                assert_eq!(hint_code(&error), Some("publication_busy"));
                assert_eq!(observed.load(Ordering::SeqCst), 3);
            }
        }
        Ok(())
    }

    /// The source projection of a publication that was replaced under the
    /// same authority is the retryable `publication_busy`, not a refusal.
    #[test]
    fn source_projection_of_a_replaced_publication_is_publication_busy() -> Result<()> {
        let roots = two_roots()?;
        let admin = &roots.admin;
        admin.index()?;
        let mut authority = fixture(&admin.root_id());
        authority.backing = airk_core::service::ProjectRegistration::new(
            admin.root_id(),
            admin.data_root.to_string_lossy().into_owned(),
            admin.collection.clone(),
        )?
        .binding;
        let backing = authority.backing;
        let scoped = admin.with_selection_reads(authority)?;
        let token = crate::current::runtime::revision(admin.current_reader()?.token()?);
        scoped.scoped_source_projection(backing, token)?;
        std::fs::write(
            std::path::Path::new(&admin.root_id()).join("main.rs"),
            "pub struct Moved;\n",
        )?;
        admin.index()?;
        let error = scoped
            .scoped_source_projection(backing, token)
            .err()
            .unwrap();
        assert!(!error.is::<ScopeReadRefused>(), "{error:#}");
        assert_eq!(hint_code(&error), Some("publication_busy"));
        Ok(())
    }

    /// Genesis: a multi-root authority whose roots are configured waits for
    /// the first publication like a single-root one; an unconfigured root is
    /// still a definite refusal.
    #[test]
    fn genesis_multi_root_scope_with_configured_roots_is_pending() -> Result<()> {
        use airk_core::graph_storage::{GraphHead, GraphScope};
        use airk_store::current::{CurrentHead, CurrentScope};
        let roots = two_roots()?;
        let admin = &roots.admin;
        admin.add_root("docs", &roots.external)?;
        let scope = CurrentScope {
            root: admin.root_id(),
            collection: admin.collection.clone(),
            incarnation: Incarnation::from_bytes([11; 16]),
        };
        let lexical = airk_store::current::lexical::LexicalManifest::empty()?;
        let head = CurrentHead {
            version: 2,
            scope: scope.clone(),
            sequence: 0,
            artifact_root: airk_store::current::map::empty_root(&scope)?,
            lexical_sequence: 0,
            lexical_commit: lexical.digest()?,
            graph_head: GraphHead::empty(GraphScope {
                root: scope.root.clone(),
                collection: scope.collection.clone(),
                incarnation: scope.incarnation,
                graph_instance: Incarnation::from_bytes([12; 16]),
                specification: Sha256Digest::from_bytes([13; 32]),
                record_bytes: 16,
            })?,
        };
        let quota = admin.collection_quota()?;
        {
            let lock = admin.store.lock()?;
            let mut current = admin.store.current_write(&lock, &quota, &scope, 16 << 20)?;
            current.initialize(head.clone())?;
            current
                .install_initial_lexical_manifest(&head.token()?, &lexical.canonical_bytes()?)?;
        }
        let configured = admin.configured_roots()?["docs"].clone();
        let multi = admin.with_selection_reads(with_external(admin, configured)?)?;
        let error = multi.status_summary().unwrap_err();
        assert!(!error.is::<ScopeReadRefused>(), "{error:#}");
        assert_eq!(hint_code(&error), Some("publication_busy"));
        let foreign = admin.with_selection_reads(with_external(
            admin,
            roots
                ._directory
                .path()
                .join("foreign")
                .to_string_lossy()
                .into_owned(),
        )?)?;
        let error = foreign.status_summary().unwrap_err();
        assert!(error.is::<ScopeReadRefused>(), "{error:#}");
        assert!(hint_code(&error).is_none());
        Ok(())
    }
}
