// SPDX-License-Identifier: Apache-2.0
//! Bounded native fact discovery from authenticated current path postings.
use super::{CurrentRead, CurrentToken, postings::Lookup};
use crate::FileStore;
use airk_core::{
    Artifact, Citation, Fact, FactHit, FactQuery,
    discovery::{
        self, CompactFact, DiscoveryKey, DiscoveryOptions, DiscoveryPage, FactDiscoveryOptions,
        MatchCount, NameMatch,
    },
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;

const MAX_FILES_PER_PAGE: usize = 256;
const MAX_EVIDENCE_BYTES: usize = 256 * 1024 * 1024;
// A cursor can advance past a file with no matching facts. Native fact ordinals
// are bounded far below this sentinel; it never denotes a source record.
const FILE_END: i64 = i64::MAX;

#[derive(Debug, Serialize)]
pub struct CurrentFactPage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimal: Option<discovery::MinimalFacts>,
    pub revision: CurrentToken,
    pub collection: String,
    pub facts: Vec<FactHit>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub outline: Vec<CompactFact>,
    #[serde(flatten)]
    pub page: DiscoveryPage,
    pub truncated: bool,
    /// These methods address native retained extraction. Imported assertions
    /// require their own source/producer binding before a runtime can combine them.
    pub evidence_scope: &'static str,
}

pub(crate) fn require_integrity(
    read: &CurrentRead,
    condition: bool,
    message: &'static str,
) -> Result<()> {
    read.observe_private_integrity(if condition {
        Ok(())
    } else {
        Err(anyhow::Error::new(super::PrivateIntegrityFailure).context(message))
    })
}

fn exact_name_match(fact: &Fact, name: &str) -> bool {
    fact.name == name
        || fact.code.as_ref().is_some_and(|code| {
            code.qualified_name == name
                || code.symbol_id == name
                || airk_core::code::terminal_call_name(code, &fact.kind) == Some(name)
        })
}

fn matches_name(fact: &Fact, q: &FactQuery, options: &FactDiscoveryOptions) -> bool {
    q.name.as_ref().is_none_or(|name| match options.name_match {
        Some(NameMatch::Exact) => exact_name_match(fact, name),
        Some(NameMatch::Suffix) => {
            fact.name.ends_with(name)
                || fact
                    .code
                    .as_ref()
                    .is_some_and(|code| code.qualified_name.ends_with(name))
        }
        Some(NameMatch::Contains) => {
            fact.name.contains(name)
                || fact
                    .code
                    .as_ref()
                    .is_some_and(|code| code.qualified_name.contains(name))
        }
        None if q.exact_name => exact_name_match(fact, name),
        None => fact.name.to_lowercase().contains(&name.to_lowercase()),
    })
}

fn matches(
    artifact: &Artifact,
    fact: &Fact,
    q: &FactQuery,
    code_only: bool,
    options: &FactDiscoveryOptions,
) -> bool {
    (!code_only || fact.code.is_some())
        && (!q.document_only
            || (fact.code.is_none() && !matches!(fact.kind.as_str(), "definition" | "import")))
        && q.kind.as_ref().is_none_or(|kind| kind == &fact.kind)
        && q.language
            .as_ref()
            .is_none_or(|lang| lang == &artifact.language)
        && matches_name(fact, q, options)
}

fn row_key(path: &str, ordinal: usize, fact: &Fact) -> DiscoveryKey {
    DiscoveryKey {
        path: path.into(),
        line: fact.start_line,
        kind: fact.kind.clone(),
        name: fact.name.clone(),
        ordinal: ordinal as i64,
    }
}

pub(crate) fn fact_hit(artifact: &Artifact, fact: &Fact) -> FactHit {
    FactHit {
        candidate: fact
            .code
            .as_ref()
            .is_some_and(|code| code.is_call_candidate(&fact.kind)),
        continuity_key: None,
        name: fact.name.clone(),
        kind: fact.kind.clone(),
        syntax_kind: fact.syntax_kind.clone(),
        language: artifact.language.clone(),
        citation: Citation {
            path: artifact.path.clone(),
            content_hash: artifact.content_hash.clone(),
            start_line: fact.start_line,
            end_line: fact.end_line,
            location: fact.location.clone(),
        },
        document: fact.document.clone(),
        code: fact.code.clone(),
    }
}

/// Merge canonical native pages with imported rows without decoding later native
/// pages after the response budget has been reached. Original ordinals remain
/// the cursor identity even though pages are stored in canonical display order.
struct ProjectedFacts<'a, 'b> {
    read: &'a CurrentRead,
    projection: &'a super::native_projection::NativeFactRead<'b>,
    page: usize,
    rows: std::vec::IntoIter<super::native_projection::NativeFactRow>,
    native: Option<super::native_projection::NativeFactRow>,
    imported: std::iter::Peekable<std::vec::IntoIter<super::native_projection::NativeFactRow>>,
}
impl Iterator for ProjectedFacts<'_, '_> {
    type Item = Result<super::native_projection::NativeFactRow>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.native.is_none() {
            loop {
                if let Some(row) = self.rows.next() {
                    self.native = Some(row);
                    break;
                }
                if self.page == self.projection.header().pages.len() {
                    break;
                }
                match self
                    .read
                    .observe_private_integrity(self.projection.page(self.page))
                {
                    Ok(page) => {
                        self.page += 1;
                        self.rows = page.rows.into_iter();
                    }
                    Err(error) => return Some(Err(error)),
                }
            }
        }
        let imported_first = match (&self.native, self.imported.peek()) {
            (Some(native), Some(imported)) => {
                row_key("", imported.ordinal, &imported.fact)
                    < row_key("", native.ordinal, &native.fact)
            }
            (None, Some(_)) => true,
            _ => false,
        };
        if imported_first {
            self.imported.next().map(Ok)
        } else {
            self.native.take().map(Ok)
        }
    }
}

impl CurrentRead {
    /// Native fact discovery retains the request gate through candidate selection,
    /// verified extraction/source reads and cursor construction. Each page caps
    /// source/extraction work independently of the response budget. Empty pages
    /// may have a continuation when scanned files contained no matching facts.
    pub fn native_facts_page(
        &self,
        store: &FileStore,
        query: &FactQuery,
        code_only: bool,
        options: &DiscoveryOptions,
    ) -> Result<CurrentFactPage> {
        self.native_facts_page_with_options(
            store,
            query,
            code_only,
            options,
            &FactDiscoveryOptions::default(),
        )
    }

    pub fn native_facts_page_with_options(
        &self,
        store: &FileStore,
        query: &FactQuery,
        code_only: bool,
        options: &DiscoveryOptions,
        fact_options: &FactDiscoveryOptions,
    ) -> Result<CurrentFactPage> {
        airk_core::validate_budget(query.limit, query.max_bytes)?;
        for (field, value) in [
            ("kind", query.kind.as_deref()),
            ("language", query.language.as_deref()),
        ] {
            if value.is_some_and(|value| value.is_empty() || value.len() > 128) {
                return Err(airk_core::read_arguments::ReadArgumentError::new("invalid_filter",field,
                    format!("{field} must contain 1..128 UTF-8 bytes; native and retained producer labels are matched exactly")).into());
            }
        }
        ensure!(
            fact_options.name_match.is_none()
                || query.name.as_ref().is_some_and(|name| !name.is_empty()),
            "name_match requires a nonempty name"
        );
        ensure!(
            !options.compact || !options.minimal,
            "choose compact facts or minimal navigation, not both"
        );
        ensure!(
            query.path_prefix.len() <= 4096
                && query.name.as_ref().is_none_or(|name| name.len() <= 1024)
                && query.kind.as_ref().is_none_or(|kind| kind.len() <= 128)
                && query
                    .language
                    .as_ref()
                    .is_none_or(|language| language.len() <= 128),
            "invalid current fact filters or compact representation"
        );
        let private = self.private_query_eligible()?;
        let projected_import = if private {
            self.observe_private_integrity(self.imported_projection(store))?
        } else {
            None
        };
        let imported = if projected_import.is_none() {
            self.analysis(store)?
        } else {
            None
        };
        let mut imported_facts = imported
            .as_ref()
            .map(|a| a.native_facts())
            .unwrap_or_default();
        let revision = self.token()?;
        let mut filters = query.clone();
        filters.limit = 0;
        filters.max_bytes = 0;
        let revision_identity =
            self.scope_bound_cursor_identity(&serde_json::to_string(&revision)?);
        let binding = if *fact_options == FactDiscoveryOptions::default() {
            // Retain existing wire/cursor identities for omitted controls.
            discovery::binding(
                &revision_identity,
                "current-native-facts/1",
                (filters, code_only, options.compact, options.minimal),
            )?
        } else {
            discovery::binding(
                &revision_identity,
                "current-native-facts/2",
                (
                    filters,
                    code_only,
                    options.compact,
                    options.minimal,
                    fact_options,
                ),
            )?
        };
        let after = discovery::decode(options.cursor.as_deref(), &binding)?;
        if let Some(after) = &after {
            ensure!(
                after.path.starts_with(&query.path_prefix)
                    && (!query.path_exact || after.path == query.path_prefix)
                    && self.artifact(&after.path)?.is_some(),
                "current fact cursor path unavailable"
            );
        }
        let exact_name = fact_options
            .name_match
            .map_or(query.exact_name, |mode| mode == NameMatch::Exact);
        let lookup = if let Some(language) = query.language.as_deref() {
            Lookup::Language(language)
        } else if let Some(name) = query.name.as_deref().filter(|_| exact_name) {
            if self.terminal_call_postings_complete()?
                || query.kind.as_deref().is_some_and(|kind| kind != "call")
            {
                Lookup::FactName(name)
            } else if query.kind.as_deref() == Some("call") {
                Lookup::FactKind("call")
            } else {
                Lookup::Files
            }
        } else if let Some(kind) = query.kind.as_deref() {
            Lookup::FactKind(kind)
        } else if let Some(language) = query.language.as_deref() {
            Lookup::Language(language)
        } else {
            Lookup::Files
        };
        // Keep legacy imported candidate-path accounting, but select only one
        // bounded page after this cursor instead of walking the full roster.
        let mut imported_paths = projected_import
            .as_ref()
            .map(|projection| {
                self.observe_private_integrity(
                    projection.lookup_paths(
                        query
                            .language
                            .as_deref()
                            .map_or(Lookup::Files, Lookup::Language),
                        &query.path_prefix,
                        if query.path_exact {
                            None
                        } else {
                            after.as_ref().map(|key| key.path.as_str())
                        },
                        if query.path_exact {
                            1
                        } else {
                            MAX_FILES_PER_PAGE + 1
                        },
                    ),
                )
            })
            .transpose()?
            .unwrap_or_default();
        if let Some(after) = after.as_ref().filter(|key| key.ordinal != FILE_END)
            && !query.path_exact
            && let Some(projection) = &projected_import
        {
            let resumed = self.observe_private_integrity(projection.lookup_paths(
                Lookup::Files,
                &after.path,
                None,
                1,
            ))?;
            imported_paths.extend(
                resumed
                    .into_iter()
                    .filter(|selected| selected.path == after.path),
            );
        }
        let (mut paths, mut more_paths) = if query.path_exact {
            (
                self.artifact(&query.path_prefix)?
                    .map(|_| vec![query.path_prefix.clone()])
                    .unwrap_or_default(),
                false,
            )
        } else {
            let page = self.lookup_paths(
                lookup,
                &query.path_prefix,
                after.as_ref().map(|key| key.path.as_str()),
                MAX_FILES_PER_PAGE,
            )?;
            (page.paths, page.next_after.is_some())
        };
        if !query.path_exact {
            for path in imported_facts.keys().filter(|path| {
                path.starts_with(&query.path_prefix)
                    && after.as_ref().is_none_or(|key| *path > &key.path)
            }) {
                let language_matches = match query.language.as_deref() {
                    None => true,
                    Some(language) => self
                        .artifact(path)?
                        .is_some_and(|artifact| artifact.language == language),
                };
                if language_matches {
                    paths.push(path.clone());
                }
            }
            paths.extend(
                imported_paths
                    .iter()
                    .filter(|selected| after.as_ref().is_none_or(|key| selected.path > key.path))
                    .map(|selected| selected.path.clone()),
            );
            paths.sort();
            paths.dedup();
            if paths.len() > MAX_FILES_PER_PAGE {
                paths.truncate(MAX_FILES_PER_PAGE);
                more_paths = true;
            }
        }
        // Resume within the last file before moving to later posting paths.
        if !query.path_exact
            && let Some(after) = after.as_ref().filter(|key| key.ordinal != FILE_END)
        {
            paths.insert(0, after.path.clone());
        }
        let mut response = CurrentFactPage {
            minimal: options.minimal.then(Default::default),
            revision,
            collection: self.head().scope.collection.clone(),
            facts: Vec::new(),
            outline: Vec::new(),
            page: DiscoveryPage {
                record_bytes: Some(0),
                ..Default::default()
            },
            truncated: more_paths,
            evidence_scope: if imported.is_some() || projected_import.is_some() {
                "native-and-imported-assertions"
            } else {
                "native"
            },
        };
        let mut last = after.clone();
        let mut matched = 0usize;
        let mut work = self.resource_reference("analysis")?.map_or(0, |r| r.bytes);
        'files: for path in paths {
            if after
                .as_ref()
                .is_some_and(|key| key.path == path && key.ordinal == FILE_END)
            {
                continue;
            }
            let compact = self
                .artifact(&path)?
                .context("current fact candidate artifact missing")?;
            if query
                .language
                .as_ref()
                .is_some_and(|language| language != &compact.language)
            {
                last = Some(DiscoveryKey {
                    path,
                    ordinal: FILE_END,
                    ..Default::default()
                });
                continue;
            }
            let retained = compact
                .retained
                .as_ref()
                .context("current fact extraction missing")?;
            let metadata_bytes = self.file_metadata_bytes(&path)?;
            let bytes = compact
                .bytes
                .checked_add(retained.object_bytes)
                .and_then(|bytes| bytes.checked_add(metadata_bytes))
                .context("current fact evidence size overflow")?;
            if bytes > MAX_EVIDENCE_BYTES.saturating_sub(work) {
                ensure!(
                    work != 0,
                    "current fact artifact exceeds request evidence budget"
                );
                response.truncated = true;
                break;
            }
            work += bytes;
            let projection = if private {
                self.observe_private_integrity(self.native_fact_projection(store, &path))?
            } else {
                None
            };
            let mut artifact = if projection.is_some() {
                compact.clone()
            } else {
                let result = (|| {
                    let artifact =
                        store.load_retained_bounded(&compact, retained.object_bytes as u64)?;
                    store.verify_retained_source(&compact)?;
                    Ok(artifact)
                })();
                if private {
                    self.observe_private_integrity(result)?
                } else {
                    result?
                }
            };
            let native_count = projection
                .as_ref()
                .map_or(artifact.facts.len(), |projection| projection.header().facts);
            if let Some(projection) = &projected_import
                && let Some(selected) = imported_paths.iter().find(|selected| selected.path == path)
            {
                require_integrity(
                    self,
                    selected.source.to_string() == compact.content_hash,
                    "current analysis source expired",
                )?;
                let mut rows = std::collections::BTreeMap::new();
                for page in &selected.pages {
                    for row in self
                        .observe_private_integrity(projection.query_page(*page))?
                        .iter()
                    {
                        if let super::query_projection::ProjectionRow::Fact {
                            path_ordinal,
                            hit,
                            ..
                        } = row
                            && hit.citation.path == path
                        {
                            require_integrity(
                                self,
                                hit.citation.content_hash == compact.content_hash,
                                "current analysis source expired",
                            )?;
                            rows.insert(
                                *path_ordinal,
                                Fact {
                                    name: hit.name.clone(),
                                    kind: hit.kind.clone(),
                                    syntax_kind: hit.syntax_kind.clone(),
                                    start_line: hit.citation.start_line,
                                    end_line: hit.citation.end_line,
                                    location: hit.citation.location.clone(),
                                    document: hit.document.clone(),
                                    code: hit.code.clone(),
                                },
                            );
                        }
                    }
                }
                for (expected, (ordinal, fact)) in rows.into_iter().enumerate() {
                    require_integrity(
                        self,
                        expected == ordinal,
                        "imported fact path ordinal missing",
                    )?;
                    artifact.facts.push(fact);
                }
            }
            if let Some(facts) = imported_facts.remove(&path) {
                self.verify_analysis_path(imported.as_ref().expect("imported facts"), &path)?;
                artifact.facts.extend(facts);
            }
            let file_metadata = self.observe_private_integrity(self.file_metadata(store, &path))?;
            let keys = airk_core::continuity::FactKeyIndex::new(
                file_metadata
                    .as_ref()
                    .map_or(&[], |file| file.continuity.as_slice()),
            );
            let mut rows = std::mem::take(&mut artifact.facts)
                .into_iter()
                .enumerate()
                .map(|(ordinal, fact)| {
                    let ordinal = if projection.is_some() {
                        native_count + ordinal
                    } else {
                        ordinal
                    };
                    let continuity_key = keys.key(ordinal, &fact);
                    super::native_projection::NativeFactRow {
                        ordinal,
                        fact,
                        continuity_key,
                    }
                })
                .collect::<Vec<_>>();
            rows.sort_by_key(|row| row_key(&path, row.ordinal, &row.fact));
            let mut first_page = 0;
            let restored = after
                .as_ref()
                .filter(|key| key.path == path)
                .map(|key| -> Result<DiscoveryKey> {
                    let ordinal = usize::try_from(key.ordinal)?;
                    if let Some(projection) = &projection
                        && ordinal < native_count
                    {
                        first_page = projection.page_for_ordinal(ordinal)?;
                        let page = self.observe_private_integrity(projection.page(first_page))?;
                        let row = page
                            .rows
                            .iter()
                            .find(|row| row.ordinal == ordinal)
                            .context("current fact cursor position unavailable")?;
                        return Ok(row_key(&path, ordinal, &row.fact));
                    }
                    let row = rows
                        .iter()
                        .find(|row| row.ordinal == ordinal)
                        .context("current fact cursor position unavailable")?;
                    Ok(row_key(&path, ordinal, &row.fact))
                })
                .transpose()?;
            let rows: Box<
                dyn Iterator<Item = Result<super::native_projection::NativeFactRow>> + '_,
            > = if let Some(projection) = &projection {
                Box::new(ProjectedFacts {
                    read: self,
                    projection,
                    page: first_page,
                    rows: Vec::new().into_iter(),
                    native: None,
                    imported: rows.into_iter().peekable(),
                })
            } else {
                Box::new(rows.into_iter().map(Ok))
            };
            for row in rows {
                let row = row?;
                let key = row_key(&path, row.ordinal, &row.fact);
                if restored.as_ref().is_some_and(|after| key <= *after) {
                    continue;
                }
                let fact = &row.fact;
                if matches(&artifact, fact, query, code_only, fact_options) {
                    // Count only records actually checked under this page's
                    // existing work bounds, including the first unadmitted hit.
                    matched = matched.saturating_add(1);
                    let mut hit = fact_hit(&artifact, fact);
                    hit.continuity_key = row.continuity_key;
                    if let Some(minimal) = response.minimal.as_mut() {
                        let Some(candidate) =
                            minimal.candidate(&hit, &artifact, query.path_exact)?
                        else {
                            response.truncated = true;
                            break 'files;
                        };
                        let used = response
                            .page
                            .record_bytes
                            .as_mut()
                            .expect("initialized above");
                        if minimal.items.len() >= query.limit
                            || candidate.bytes > query.max_bytes.saturating_sub(*used)
                        {
                            response.truncated = true;
                            if minimal.items.is_empty() {
                                response.page.required_record_bytes = Some(candidate.bytes);
                            }
                            break 'files;
                        }
                        *used += candidate.bytes;
                        minimal.admit(candidate)?;
                        last = Some(key);
                        continue;
                    }
                    let compact_hit = options.compact.then(|| CompactFact::from(&hit));
                    let bytes = match &compact_hit {
                        Some(hit) => serde_json::to_vec(hit)?.len(),
                        None => serde_json::to_vec(&hit)?.len(),
                    };
                    let count = response.facts.len() + response.outline.len();
                    let used = response
                        .page
                        .record_bytes
                        .as_mut()
                        .expect("initialized above");
                    if count == query.limit || bytes > query.max_bytes.saturating_sub(*used) {
                        response.truncated = true;
                        if count == 0 {
                            response.page.required_record_bytes = Some(bytes);
                        }
                        break 'files;
                    }
                    *used += bytes;
                    if let Some(hit) = compact_hit {
                        response.outline.push(hit);
                    } else {
                        response.facts.push(hit);
                    }
                }
                last = Some(key);
            }
            last = Some(DiscoveryKey {
                path,
                ordinal: FILE_END,
                ..Default::default()
            });
        }
        if response.truncated {
            response.page.cursor = last
                .as_ref()
                .map(|key| discovery::encode(&binding, key))
                .transpose()?;
        }
        if after.is_none() && !response.truncated && matched == 0 {
            for (field, value) in [
                ("kind", query.kind.as_deref()),
                ("language", query.language.as_deref()),
            ] {
                if let Some(value) = value {
                    response.page.filter_observations.push(
                        airk_core::discovery::FilterObservation::no_matching_records(field, value),
                    );
                }
            }
        }
        if fact_options.include_count {
            let complete = after.is_none() && !response.truncated;
            response.page.match_count = Some(MatchCount {
                total_matches: complete.then_some(matched),
                lower_bound: matched,
                complete,
                scope: if complete { "query" } else { "page_scan" }.into(),
            });
        }
        self.token()?;
        Ok(response)
    }
}
