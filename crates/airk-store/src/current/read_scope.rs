// SPDX-License-Identifier: Apache-2.0
//! Immutable authority on one request-held current reader.
use super::CurrentRead;
use airk_core::{
    identity::Sha256Digest,
    scoped_reads::{CompiledReadScope, ScopeReadRefused},
};
use anyhow::{Result, ensure};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

const ORIGIN_PATHS: usize = 128;
const ORIGIN_PAGES: usize = 128;
const ORIGIN_BYTES: usize = 8 << 20;
const ORIGIN_ROWS: usize = 32768;
#[derive(Default)]
pub(super) struct OriginBudget {
    lookups: usize,
    paths: usize,
    pages: usize,
    bytes: usize,
    rows: usize,
    native: BTreeMap<String, Arc<BTreeSet<String>>>,
    projection_pages:
        BTreeMap<(Sha256Digest, usize), Arc<Vec<super::query_projection::ProjectionRow>>>,
    analysis: Option<Arc<BTreeSet<String>>>,
}

impl CurrentRead {
    pub fn attach_read_scope_fence(
        &self,
        scope: Arc<CompiledReadScope>,
        revoked: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<()> {
        self.attach_read_scope(scope)?;
        if let Err(revoked) = self.read_scope_fence.set(revoked) {
            ensure!(
                self.read_scope_fence
                    .get()
                    .is_some_and(|existing| Arc::ptr_eq(existing, &revoked)),
                ScopeReadRefused
            );
        }
        self.require_read_scope_live()
    }

    pub(crate) fn require_read_scope_live(&self) -> Result<()> {
        self.require_source_policy_live()?;
        ensure!(
            !self
                .read_scope_fence
                .get()
                .is_some_and(|fence| fence.load(std::sync::atomic::Ordering::Acquire)),
            ScopeReadRefused
        );
        Ok(())
    }
    /// Attach before exposing the reader. A retained reader can never widen its scope.
    pub fn attach_read_scope(&self, scope: Arc<CompiledReadScope>) -> Result<()> {
        scope.validate()?;
        if let Some(existing) = self.read_scope.get() {
            ensure!(existing.as_ref() == scope.as_ref(), ScopeReadRefused);
            return Ok(());
        }
        let restricts = !self.collection_state()?.is_some_and(|state| {
            scope.covers_sources(&self.head().scope.root, &state.configuration.roots)
        });
        if let Err(restricts) = self.read_scope_restricts_paths.set(restricts) {
            ensure!(
                self.read_scope_restricts_paths.get() == Some(&restricts),
                ScopeReadRefused
            );
        }
        if let Err(scope) = self.read_scope.set(scope) {
            ensure!(
                self.read_scope
                    .get()
                    .is_some_and(|existing| existing.as_ref() == scope.as_ref()),
                ScopeReadRefused
            );
        }
        Ok(())
    }

    pub fn accepts_path(&self, path: &str) -> bool {
        self.require_read_scope_live().is_ok()
            && self.source_policy_allows_path(path)
            && (!self.restricts_paths()
                || self
                    .read_scope
                    .get()
                    .is_none_or(|scope| scope.allows_path(path)))
    }
    /// Retains authority identity/revocation while avoiding a path roster walk
    /// for a frozen scope covering every source of this exact publication.
    pub fn restricts_paths(&self) -> bool {
        self.source_policy
            .get()
            .is_some_and(|policy| policy.restricts)
            || self.selected_scope_restricts_paths()
    }
    /// Selected Project coverage of this exact publication, independently of
    /// permanent source-policy filtering. Read scope attachment computes this
    /// once from the published root/alias configuration; it grants no path access.
    pub fn selected_scope_restricts_paths(&self) -> bool {
        self.read_scope.get().is_some()
            && self
                .read_scope_restricts_paths
                .get()
                .copied()
                .unwrap_or(true)
    }

    pub fn require_path(&self, path: &str) -> Result<()> {
        ensure!(self.accepts_path(path), ScopeReadRefused);
        Ok(())
    }

    pub fn read_scope_fingerprint(&self) -> Option<Sha256Digest> {
        self.read_scope.get().map(|scope| scope.fingerprint)
    }

    pub fn scope_bound_cursor_identity(&self, original: &str) -> String {
        self.read_scope_fingerprint().map_or_else(
            || original.to_owned(),
            |fingerprint| format!("{original}:scope:{fingerprint}"),
        )
    }

    pub(crate) fn accepts_endpoint(
        &self,
        store: &crate::FileStore,
        endpoint: &str,
    ) -> Result<bool> {
        if !self.restricts_paths() {
            return Ok(true);
        }
        self.require_read_scope_live()?;
        if let Some(path) = endpoint.strip_prefix("file:") {
            return Ok(self.accepts_path(path));
        }
        ensure!(endpoint.len() <= 4096, ScopeReadRefused);
        let cache = self
            .scope_endpoints
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| ScopeReadRefused)?;
        if let Some(allowed) = cache.get(endpoint) {
            return Ok(*allowed);
        }
        ensure!(cache.len() < 4096, ScopeReadRefused);
        drop(cache); // Definition proof must never hold the result-cache mutex during I/O.
        {
            let mut budget = self
                .scope_origin_budget
                .get_or_init(Default::default)
                .lock()
                .map_err(|_| ScopeReadRefused)?;
            ensure!(budget.lookups < 128, ScopeReadRefused);
            budget.lookups += 1;
        }
        let paths = self.lookup_paths(
            super::postings::Lookup::FactName(endpoint),
            "",
            None,
            ORIGIN_PATHS + 1,
        )?;
        ensure!(
            paths.next_after.is_none() && paths.paths.len() <= ORIGIN_PATHS,
            ScopeReadRefused
        );
        self.charge_scope_origin(paths.paths.len(), 0, 0, 0)?;
        let mut allowed = false;
        for path in paths.paths {
            if self
                .native_definition_origins(store, &path)?
                .contains(endpoint)
            {
                allowed = true;
                break;
            }
        }
        if !allowed && let Some(imported) = self.imported_projection(store)? {
            allowed = imported.definition_origin(endpoint)?;
        } else if !allowed {
            allowed = self.analysis_definition_origins(store)?.contains(endpoint);
        }
        let mut endpoints = self
            .scope_endpoints
            .get()
            .ok_or(ScopeReadRefused)?
            .lock()
            .map_err(|_| ScopeReadRefused)?;
        ensure!(endpoints.len() < 4096, ScopeReadRefused);
        endpoints.insert(endpoint.to_owned(), allowed);
        Ok(allowed)
    }

    pub(crate) fn charge_scope_origin(
        &self,
        paths: usize,
        pages: usize,
        bytes: usize,
        rows: usize,
    ) -> Result<()> {
        let mut budget = self
            .scope_origin_budget
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| ScopeReadRefused)?;
        ensure!(
            paths <= ORIGIN_PATHS.saturating_sub(budget.paths)
                && pages <= ORIGIN_PAGES.saturating_sub(budget.pages)
                && bytes <= ORIGIN_BYTES.saturating_sub(budget.bytes)
                && rows <= ORIGIN_ROWS.saturating_sub(budget.rows),
            ScopeReadRefused
        );
        budget.paths += paths;
        budget.pages += pages;
        budget.bytes += bytes;
        budget.rows += rows;
        Ok(())
    }
    fn native_definition_origins(
        &self,
        store: &crate::FileStore,
        path: &str,
    ) -> Result<Arc<BTreeSet<String>>> {
        if let Some(origins) = self
            .scope_origin_budget
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| ScopeReadRefused)?
            .native
            .get(path)
            .cloned()
        {
            return Ok(origins);
        }
        let compact = self.artifact(path)?.ok_or(ScopeReadRefused)?;
        let retained = compact.retained.as_ref().ok_or(ScopeReadRefused)?;
        self.charge_scope_origin(0, 0, retained.object_bytes, 0)?;
        let artifact = store.load_retained_bounded(&compact, retained.object_bytes as u64)?;
        self.charge_scope_origin(0, 0, 0, artifact.facts.len())?;
        let origins = Arc::new(
            artifact
                .facts
                .iter()
                .filter(|fact| fact.kind == "definition")
                .filter_map(|fact| fact.code.as_ref())
                .map(|code| code.symbol_id.clone())
                .collect::<BTreeSet<_>>(),
        );
        ensure!(
            origins.len() <= 4096 && origins.iter().all(|id| id.len() <= 4096),
            ScopeReadRefused
        );
        self.scope_origin_budget
            .get()
            .ok_or(ScopeReadRefused)?
            .lock()
            .map_err(|_| ScopeReadRefused)?
            .native
            .insert(path.into(), origins.clone());
        Ok(origins)
    }
    pub(crate) fn origin_projection_page(
        &self,
        store: &crate::FileStore,
        reference: &super::resources::ResourceReference,
        page: usize,
    ) -> Result<Arc<Vec<super::query_projection::ProjectionRow>>> {
        let key = (reference.digest, page);
        if let Some(rows) = self
            .scope_origin_budget
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| ScopeReadRefused)?
            .projection_pages
            .get(&key)
            .cloned()
        {
            return Ok(rows);
        }
        self.charge_scope_origin(0, 1, reference.bytes, 0)?;
        let rows = Arc::new(store.read_projection_page(reference, page)?);
        self.scope_origin_budget
            .get()
            .ok_or(ScopeReadRefused)?
            .lock()
            .map_err(|_| ScopeReadRefused)?
            .projection_pages
            .insert(key, rows.clone());
        Ok(rows)
    }
    pub(crate) fn install_analysis_definition_origins(
        &self,
        facts: &[airk_core::FactHit],
        bytes: usize,
    ) -> Result<Arc<BTreeSet<String>>> {
        if let Some(origins) = self
            .scope_origin_budget
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| ScopeReadRefused)?
            .analysis
            .clone()
        {
            return Ok(origins);
        }
        self.charge_scope_origin(0, 0, bytes, facts.len())?;
        let origins = Arc::new(
            facts
                .iter()
                .filter(|hit| hit.kind == "definition" && self.accepts_path(&hit.citation.path))
                .filter_map(|hit| hit.code.as_ref())
                .map(|code| code.symbol_id.clone())
                .collect::<BTreeSet<_>>(),
        );
        ensure!(
            origins.len() <= 4096 && origins.iter().all(|id| id.len() <= 4096),
            ScopeReadRefused
        );
        self.scope_origin_budget
            .get()
            .ok_or(ScopeReadRefused)?
            .lock()
            .map_err(|_| ScopeReadRefused)?
            .analysis = Some(origins.clone());
        Ok(origins)
    }
    fn analysis_definition_origins(
        &self,
        store: &crate::FileStore,
    ) -> Result<Arc<BTreeSet<String>>> {
        if let Some(origins) = self
            .scope_origin_budget
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| ScopeReadRefused)?
            .analysis
            .clone()
        {
            return Ok(origins);
        }
        let Some(reference) = self.resource_reference("analysis")? else {
            return Ok(Arc::new(BTreeSet::new()));
        };
        // This fallback reads the original authenticated analysis, never analysis()
        // whose record admission itself requires these definition proofs.
        self.charge_scope_origin(0, 0, reference.bytes, 0)?;
        let bytes = store.read_current_resource(&reference)?;
        let evidence: super::analysis::AnalysisEvidence = serde_json::from_slice(&bytes)?;
        ensure!(
            evidence.canonical_bytes()? == bytes
                && evidence.manifest.root == self.head().scope.root
                && evidence.manifest.collection == self.head().scope.collection,
            "current analysis ownership mismatch"
        );
        self.install_analysis_definition_origins(&evidence.facts, 0)
    }
    pub(crate) fn require_imported_fact(
        &self,
        store: &crate::FileStore,
        hit: &airk_core::FactHit,
    ) -> Result<()> {
        if !self.restricts_paths() {
            return Ok(());
        }
        let Some(code) = &hit.code else {
            return Ok(());
        };
        // The physical imported component is the trust boundary. An indexed
        // assertion cannot exempt itself by claiming a native evidence basis.
        self.require_path(&hit.citation.path)?;
        if hit.kind != "definition" {
            ensure!(
                self.accepts_endpoint(store, &code.symbol_id)?,
                ScopeReadRefused
            );
        }
        for endpoint in [code.owner_id.as_deref(), code.target.as_deref()]
            .into_iter()
            .flatten()
        {
            ensure!(self.accepts_endpoint(store, endpoint)?, ScopeReadRefused);
        }
        Ok(())
    }

    pub(crate) fn scope_atoms_json(&self) -> Result<Option<String>> {
        if !self.restricts_paths() {
            return Ok(None);
        }
        self.read_scope
            .get()
            .map(|scope| serde_json::to_string(&scope.atoms).map_err(Into::into))
            .transpose()
    }

    pub(crate) fn scope_aliases_json(&self) -> Result<Option<String>> {
        if !self.restricts_paths() {
            return Ok(None);
        }
        self.read_scope
            .get()
            .map(|scope| serde_json::to_string(&scope.external_aliases).map_err(Into::into))
            .transpose()
    }
}
