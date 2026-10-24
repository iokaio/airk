// SPDX-License-Identifier: Apache-2.0
//! Bounded native fact discovery from authenticated current path postings.
use super::{CurrentRead, CurrentToken, postings::Lookup};
use crate::FileStore;
use airk_core::{
    Artifact, Citation, Fact, FactHit, FactQuery,
    discovery::{self, CompactFact, DiscoveryKey, DiscoveryOptions, DiscoveryPage},
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

fn matches(artifact: &Artifact, fact: &Fact, q: &FactQuery, code_only: bool) -> bool {
    (!code_only || fact.code.is_some())
        && (!q.document_only
            || (fact.code.is_none() && !matches!(fact.kind.as_str(), "definition" | "import")))
        && q.kind.as_ref().is_none_or(|kind| kind == &fact.kind)
        && q.language
            .as_ref()
            .is_none_or(|lang| lang == &artifact.language)
        && q.name.as_ref().is_none_or(|name| {
            if q.exact_name {
                &fact.name == name
                    || fact
                        .code
                        .as_ref()
                        .is_some_and(|code| &code.qualified_name == name || &code.symbol_id == name)
            } else {
                fact.name.to_lowercase().contains(&name.to_lowercase())
            }
        })
}

fn order_key(artifact: &Artifact, ordinal: usize) -> DiscoveryKey {
    let fact = &artifact.facts[ordinal];
    DiscoveryKey {
        path: artifact.path.clone(),
        line: fact.start_line,
        kind: fact.kind.clone(),
        name: fact.name.clone(),
        ordinal: ordinal as i64,
    }
}

pub(crate) fn fact_hit(artifact: &Artifact, fact: &Fact) -> FactHit {
    FactHit {
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
        airk_core::validate_budget(query.limit, query.max_bytes)?;
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
        let imported = self.analysis(store)?;
        let mut imported_facts = imported
            .as_ref()
            .map(|a| a.native_facts())
            .unwrap_or_default();
        let revision = self.token()?;
        let mut filters = query.clone();
        filters.limit = 0;
        filters.max_bytes = 0;
        let binding = discovery::binding(
            &serde_json::to_string(&revision)?,
            "current-native-facts/1",
            (filters, code_only, options.compact, options.minimal),
        )?;
        let after = discovery::decode(options.cursor.as_deref(), &binding)?;
        if let Some(after) = &after {
            ensure!(
                after.path.starts_with(&query.path_prefix)
                    && (!query.path_exact || after.path == query.path_prefix)
                    && self.artifact(&after.path)?.is_some(),
                "current fact cursor path unavailable"
            );
        }
        let lookup = if let Some(name) = query.name.as_deref().filter(|_| query.exact_name) {
            Lookup::FactName(name)
        } else if let Some(kind) = query.kind.as_deref() {
            Lookup::FactKind(kind)
        } else if let Some(language) = query.language.as_deref() {
            Lookup::Language(language)
        } else {
            Lookup::Files
        };
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
            paths.extend(
                imported_facts
                    .keys()
                    .filter(|path| {
                        path.starts_with(&query.path_prefix)
                            && after.as_ref().is_none_or(|key| *path > &key.path)
                    })
                    .cloned(),
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
            evidence_scope: if imported.is_some() {
                "native-and-imported-assertions"
            } else {
                "native"
            },
        };
        let mut last = after.clone();
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
            let mut artifact =
                store.load_retained_bounded(&compact, retained.object_bytes as u64)?;
            store.verify_retained_source(&compact)?;
            if let Some(facts) = imported_facts.remove(&path) {
                self.verify_analysis_path(imported.as_ref().expect("imported facts"), &path)?;
                artifact.facts.extend(facts);
            }
            let file_metadata = self.file_metadata(store, &path)?;
            let keys = airk_core::continuity::FactKeyIndex::new(
                file_metadata
                    .as_ref()
                    .map_or(&[], |file| file.continuity.as_slice()),
            );
            let restored = after
                .as_ref()
                .filter(|key| key.path == path)
                .map(|key| {
                    let ordinal = usize::try_from(key.ordinal)?;
                    ensure!(
                        ordinal < artifact.facts.len(),
                        "current fact cursor position unavailable"
                    );
                    Ok::<_, anyhow::Error>(order_key(&artifact, ordinal))
                })
                .transpose()?;
            let mut ordinals = (0..artifact.facts.len()).collect::<Vec<_>>();
            ordinals.sort_by(|a, b| {
                let a_fact = &artifact.facts[*a];
                let b_fact = &artifact.facts[*b];
                (a_fact.start_line, &a_fact.kind, &a_fact.name, a).cmp(&(
                    b_fact.start_line,
                    &b_fact.kind,
                    &b_fact.name,
                    b,
                ))
            });
            for ordinal in ordinals {
                let key = order_key(&artifact, ordinal);
                if restored.as_ref().is_some_and(|after| key <= *after) {
                    continue;
                }
                let fact = &artifact.facts[ordinal];
                if matches(&artifact, fact, query, code_only) {
                    let mut hit = fact_hit(&artifact, fact);
                    hit.continuity_key = keys.key(ordinal, fact);
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
        Ok(response)
    }
}
