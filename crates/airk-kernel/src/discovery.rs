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
            !options.compact && !options.minimal,
            "compact representation is supported by outline only"
        );
        let read = self.current_reader()?;
        let publication = Some(current::runtime::envelope(&read)?);
        let binding = discovery::binding(
            &publication
                .as_ref()
                .context("indexed publication missing; rebuild with airk index --full")?
                .token
                .to_string(),
            "list_files",
            (&self.collection, prefix),
        )?;
        let after = discovery::decode(options.cursor.as_deref(), &binding)?;
        if let Some(after) = &after {
            ensure!(
                after.path.starts_with(prefix) && read.artifact(&after.path)?.is_some(),
                "discovery cursor position is unavailable"
            );
        }
        let mut response = FileList {
            page: Default::default(),
            publication,
            generation: read.head().sequence,
            collection: self.collection.clone(),
            paths: Vec::new(),
            truncated: false,
        };
        response.page.record_bytes = Some(0);
        let mut last = after.clone();
        let candidates = read.lookup_paths(
            airk_store::current::postings::Lookup::Files,
            prefix,
            after.as_ref().map(|k| k.path.as_str()),
            limit + 1,
        )?;
        for path in &candidates.paths {
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
        self.current_facts(self.current_reader()?.as_ref(), query, false, options)
    }

    pub fn code_facts(&self, query: &FactQuery) -> Result<FactsResponse> {
        self.code_facts_page(query, &DiscoveryOptions::default())
    }

    pub fn code_facts_page(
        &self,
        query: &FactQuery,
        options: &DiscoveryOptions,
    ) -> Result<FactsResponse> {
        self.current_facts(self.current_reader()?.as_ref(), query, true, options)
    }

    pub fn outline(&self, path: &str, limit: usize, max_bytes: usize) -> Result<FactsResponse> {
        let read = self.current_reader()?;
        ensure!(read.artifact(path)?.is_some(), "path is not indexed");
        // Full exact file lookup, not a prefix that could include neighboring files.
        self.current_facts(
            &read,
            &FactQuery {
                path_prefix: path.into(),
                name: None,
                kind: None,
                language: None,
                exact_name: false,
                limit,
                max_bytes,
                path_exact: true,
                document_only: false,
            },
            false,
            &DiscoveryOptions::default(),
        )
    }
}
