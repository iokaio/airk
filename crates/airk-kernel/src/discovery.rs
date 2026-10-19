// SPDX-License-Identifier: Apache-2.0
//! Bounded inventory and fact discovery.
use super::*;
use airk_core::discovery::{self, DiscoveryKey, DiscoveryOptions};

impl Kernel {
    pub fn list_files(&self, prefix: &str, limit: usize, max_bytes: usize) -> Result<FileList> {
        self.list_files_page(prefix, limit, max_bytes, &DiscoveryOptions::default())
    }

    pub fn list_files_page(
        &self,
        prefix: &str,
        limit: usize,
        max_bytes: usize,
        options: &DiscoveryOptions,
    ) -> Result<FileList> {
        validate_budget(limit, max_bytes)?;
        ensure!(prefix.len() <= 4096, "prefix exceeds 4096 bytes");
        ensure!(
            !options.compact,
            "compact representation is supported by outline only"
        );
        let catalog = self.catalog()?;
        let snapshot = &catalog.snapshot;
        let publication = Self::catalog_envelope(&catalog);
        let binding = discovery::binding(
            &publication
                .as_ref()
                .context("indexed publication missing; rebuild with airk index --full")?
                .token
                .to_string(),
            "list_files",
            (&snapshot.collection, prefix),
        )?;
        let after = discovery::decode(options.cursor.as_deref(), &binding)?;
        if let Some(after) = &after {
            ensure!(
                after.path.starts_with(prefix) && snapshot.artifacts.contains_key(&after.path),
                "discovery cursor position is unavailable"
            );
        }
        let mut response = FileList {
            page: Default::default(),
            publication,
            generation: snapshot.generation,
            collection: snapshot.collection.clone(),
            paths: Vec::new(),
            truncated: false,
        };
        response.page.record_bytes = Some(0);
        let mut last = after.clone();
        use std::ops::Bound::{Excluded, Included, Unbounded};
        let lower = after.as_ref().map_or_else(
            || Included(prefix.to_owned()),
            |key| Excluded(key.path.clone()),
        );
        for (path, _) in snapshot.artifacts.range((lower, Unbounded)) {
            if !path.starts_with(prefix) {
                break;
            }
            let bytes = serde_json::to_vec(path)?.len();
            let used = response
                .page
                .record_bytes
                .as_mut()
                .expect("record accounting initialized");
            if response.paths.len() >= limit || bytes > max_bytes.saturating_sub(*used) {
                response.truncated = true;
                if response.paths.is_empty() {
                    response.page.required_record_bytes = Some(bytes);
                }
                break;
            }
            *used += bytes;
            response.paths.push(path.clone());
            last = Some(DiscoveryKey {
                path: path.clone(),
                ..Default::default()
            });
        }
        if response.truncated && response.publication.is_some() {
            response.page.cursor = last
                .as_ref()
                .map(|key| discovery::encode(&binding, key))
                .transpose()?;
        }
        Ok(response)
    }

    pub fn facts(&self, query: &FactQuery) -> Result<FactsResponse> {
        self.facts_page(query, &DiscoveryOptions::default())
    }

    pub fn facts_page(
        &self,
        query: &FactQuery,
        options: &DiscoveryOptions,
    ) -> Result<FactsResponse> {
        validate_budget(query.limit, query.max_bytes)?;
        ensure!(
            query.path_prefix.len() <= 4096
                && query.name.as_ref().is_none_or(|n| n.len() <= 1024)
                && query.kind.as_ref().is_none_or(|kind| kind.len() <= 128)
                && query
                    .language
                    .as_ref()
                    .is_none_or(|language| language.len() <= 128),
            "fact filter too long"
        );
        self.require_current_index()?;
        self.store
            .facts_page(&self.root_id(), &self.collection, query, false, options)?
            .context("indexed fact catalog missing; rebuild with airk index --full")
    }

    pub fn code_facts(&self, query: &FactQuery) -> Result<FactsResponse> {
        self.code_facts_page(query, &DiscoveryOptions::default())
    }

    pub fn code_facts_page(
        &self,
        query: &FactQuery,
        options: &DiscoveryOptions,
    ) -> Result<FactsResponse> {
        validate_budget(query.limit, query.max_bytes)?;
        ensure!(
            query.path_prefix.len() <= 4096
                && query.name.as_ref().is_none_or(|n| n.len() <= 1024)
                && query.kind.as_ref().is_none_or(|k| k.len() <= 128)
                && query.language.as_ref().is_none_or(|l| l.len() <= 128),
            "code fact filter too long"
        );
        self.require_current_index()?;
        self.store
            .facts_page(&self.root_id(), &self.collection, query, true, options)?
            .context("indexed code fact catalog missing; rebuild with airk index --full")
    }

    pub fn outline(&self, path: &str, limit: usize, max_bytes: usize) -> Result<FactsResponse> {
        self.file_info(path)?;
        // Full exact file lookup, not a prefix that could include neighboring files.
        self.facts(&FactQuery {
            path_prefix: path.into(),
            name: None,
            kind: None,
            language: None,
            exact_name: false,
            limit,
            max_bytes,
            path_exact: true,
            document_only: false,
        })
    }
}
