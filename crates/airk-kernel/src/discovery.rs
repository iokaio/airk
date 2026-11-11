// SPDX-License-Identifier: Apache-2.0
//! Bounded inventory and fact discovery.
use super::*;
use airk_core::discovery::{self, DiscoveryKey, DiscoveryOptions, FactDiscoveryOptions};

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
            &self.scope_bound_cursor_identity(
                &publication
                    .as_ref()
                    .context("indexed publication missing; rebuild with airk index --full")?
                    .token
                    .to_string(),
            )?,
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
        let candidates = read.lookup_paths(
            airk_store::current::postings::Lookup::Files,
            prefix,
            after.as_ref().map(|k| k.path.as_str()),
            limit + 1,
        )?;
        for path in candidates.paths {
            // The count limit already proves truncation; no record-byte hint is needed.
            if response.paths.len() >= limit {
                response.truncated = true;
                break;
            }
            let bytes = json_string_bytes(&path);
            let used = response
                .page
                .record_bytes
                .as_mut()
                .expect("record accounting initialized");
            if bytes > max_bytes.saturating_sub(*used) {
                response.truncated = true;
                if response.paths.is_empty() {
                    response.page.required_record_bytes = Some(bytes);
                }
                break;
            }
            *used += bytes;
            response.paths.push(path);
        }
        if response.truncated && response.publication.is_some() {
            let last = response
                .paths
                .last()
                .map(|path| DiscoveryKey {
                    path: path.clone(),
                    ..Default::default()
                })
                .or(after);
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
        self.facts_page_with_options(query, options, &FactDiscoveryOptions::default())
    }

    pub fn facts_page_with_options(
        &self,
        query: &FactQuery,
        options: &DiscoveryOptions,
        fact_options: &FactDiscoveryOptions,
    ) -> Result<FactsResponse> {
        self.current_facts_with_options(
            self.current_reader()?.as_ref(),
            query,
            false,
            options,
            fact_options,
        )
    }

    pub fn code_facts(&self, query: &FactQuery) -> Result<FactsResponse> {
        self.code_facts_page(query, &DiscoveryOptions::default())
    }

    pub fn code_facts_page(
        &self,
        query: &FactQuery,
        options: &DiscoveryOptions,
    ) -> Result<FactsResponse> {
        self.code_facts_page_with_options(query, options, &FactDiscoveryOptions::default())
    }

    pub fn code_facts_page_with_options(
        &self,
        query: &FactQuery,
        options: &DiscoveryOptions,
        fact_options: &FactDiscoveryOptions,
    ) -> Result<FactsResponse> {
        self.current_facts_with_options(
            self.current_reader()?.as_ref(),
            query,
            true,
            options,
            fact_options,
        )
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

/// Exact serde_json string size for an inventory record, including its two quotes.
/// UTF-8 bytes are preserved; only JSON syntax and ASCII controls are escaped.
fn json_string_bytes(value: &str) -> usize {
    value.bytes().fold(2, |bytes, byte| {
        bytes
            + match byte {
                0x22 | 0x5c | 0x08 | 0x09 | 0x0a | 0x0c | 0x0d => 2,
                0..=0x1f => 6,
                _ => 1,
            }
    })
}

#[cfg(test)]
mod tests {
    use super::json_string_bytes;

    #[test]
    fn inventory_json_size_matches_independent_serializer_for_controls_and_unicode() {
        let mut values = vec![
            String::new(),
            "plain/path.txt".into(),
            r#"quotes"slash/\tail"#.into(),
            "é中😀\u{2028}\u{2029}".into(),
            "\u{7f}\u{80}\u{7ff}\u{800}\u{ffff}\u{10000}\u{10ffff}".into(),
        ];
        for value in 0u8..=127 {
            values.push(format!("prefix{}suffix", char::from(value)));
        }
        values.push((0u8..=31).map(char::from).collect());
        for value in values {
            assert_eq!(
                json_string_bytes(&value),
                serde_json::to_vec(&value).unwrap().len(),
                "{value:?}"
            );
        }
    }
}
