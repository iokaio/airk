// SPDX-License-Identifier: Apache-2.0
//! Isolated O2b experiment. No active publication path calls this module.
//! Independent charged copies only; never mutate or hardlink a pinned generation.
use crate::{
    bounded::{BoundedDirectory, BudgetWriter, Charge},
    persistent::{ManifestV2, PersistentIndex},
    terms, truncate_utf8,
};
use airk_core::{
    ArtifactSource, Citation, Hit, SearchQuery, SearchResponse,
    changes::LineageEnvelope,
    identity::{PublicationReceipt, RevisionToken, Sha256Digest, revision_digest},
    publication::InventoryV2,
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};
use tantivy::{
    Index, IndexReader, Order, ReloadPolicy, TantivyDocument, Term,
    collector::TopDocs,
    query::{BooleanQuery, Occur, Query, RegexQuery, TermQuery},
    schema::{FAST, INDEXED, IndexRecordOption, STORED, STRING, Schema, Value},
};

#[derive(Debug, Serialize)]
pub struct ChangedLexicalInput {
    pub base: RevisionToken,
    pub target: RevisionToken,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub replaced: Vec<String>,
    /// Unchanged files also require replacement when earlier chunk counts move
    /// their global ordinal. Ignoring this would change query ordering.
    pub ordinal_shifted: Vec<String>,
    pub full_rebuild_reasons: Vec<&'static str>,
}

fn starts(inventory: &InventoryV2) -> Result<BTreeMap<String, u64>> {
    let mut ordinal = 0u64;
    let mut result = BTreeMap::new();
    for (path, file) in &inventory.files {
        result.insert(path.clone(), ordinal);
        ordinal = ordinal
            .checked_add(file.chunks as u64)
            .context("ordinal overflow")?;
    }
    Ok(result)
}

pub fn changed_input(
    old: &InventoryV2,
    next: &InventoryV2,
    base: RevisionToken,
    target: RevisionToken,
) -> Result<ChangedLexicalInput> {
    old.validate()?;
    next.validate()?;
    ensure!(
        base.incarnation == target.incarnation
            && target.generation > base.generation
            && old.incarnation == base.incarnation
            && next.incarnation == target.incarnation
            && old.generation == base.generation
            && next.generation == target.generation
            && old.collection == next.collection,
        "incremental lexical scope mismatch"
    );
    ensure!(
        old.files.len() <= 100_000 && next.files.len() <= 100_000,
        "lexical experiment file bound"
    );
    let mut result = ChangedLexicalInput {
        base,
        target,
        added: vec![],
        removed: vec![],
        replaced: vec![],
        ordinal_shifted: vec![],
        full_rebuild_reasons: vec![],
    };
    if old.root != next.root || old.roots != next.roots {
        result.full_rebuild_reasons.push("roots");
    }
    if old.profiles != next.profiles {
        result.full_rebuild_reasons.push("profiles");
    }
    if old.extractor_version != next.extractor_version {
        result.full_rebuild_reasons.push("extractor");
    }
    if old.version != next.version || old.retrieval_specification != next.retrieval_specification {
        result.full_rebuild_reasons.push("schema");
    }
    if old.embedding_model.is_some() || next.embedding_model.is_some() {
        result.full_rebuild_reasons.push("vector_row_identity");
    }
    if serde_json::to_vec(&(
        &old.analysis,
        &old.imported_facts,
        &old.imported_relationships,
    ))? != serde_json::to_vec(&(
        &next.analysis,
        &next.imported_facts,
        &next.imported_relationships,
    ))? {
        result.full_rebuild_reasons.push("producer_or_import");
    }
    let old_starts = starts(old)?;
    let next_starts = starts(next)?;
    for (path, file) in &next.files {
        match old.files.get(path) {
            None => result.added.push(path.clone()),
            Some(previous) if previous != file => result.replaced.push(path.clone()),
            Some(_) if old_starts[path] != next_starts[path] => {
                result.ordinal_shifted.push(path.clone())
            }
            Some(_) => (),
        }
    }
    result.removed.extend(
        old.files
            .keys()
            .filter(|path| !next.files.contains_key(*path))
            .cloned(),
    );
    ensure!(
        serde_json::to_vec(&result)?.len() <= 16 * 1024 * 1024,
        "lexical change plan byte bound"
    );
    Ok(result)
}

#[derive(Debug, Serialize)]
pub struct ExperimentMetrics {
    pub input: ChangedLexicalInput,
    pub elapsed_micros: u128,
    pub copied_bytes: u64,
    /// Cumulative allocation, including rewritten metadata and merge intermediates.
    pub charged_bytes: u64,
    pub output_bytes: u64,
    pub loaded_artifacts: usize,
}

pub struct ExperimentRequest<'a> {
    pub collection_directory: &'a Path,
    pub base_receipt: &'a PublicationReceipt,
    pub base_inventory: &'a InventoryV2,
    pub inventory: &'a InventoryV2,
    pub lineage: &'a LineageEnvelope,
    pub target: RevisionToken,
    pub source: &'a dyn ArtifactSource,
    pub staging_parent: &'a Path,
    pub charge: Charge,
}

pub struct PreparedLexical {
    reader: IndexReader,
    // Drop the reader before its private directory (important on Windows).
    directory: tempfile::TempDir,
    target: RevisionToken,
    collection: String,
    pub metrics: ExperimentMetrics,
}

fn raw(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}
fn expected_schema() -> Schema {
    let mut schema = Schema::builder();
    schema.add_text_field("terms", STRING);
    schema.add_text_field("path", STRING);
    schema.add_u64_field("ordinal", FAST | INDEXED);
    schema.add_text_field("payload", STORED);
    schema.build()
}
fn read_manifest(path: &Path, receipt: &PublicationReceipt) -> Result<ManifestV2> {
    let metadata = path.symlink_metadata()?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 131072,
        "invalid base manifest file"
    );
    let mut bytes = vec![];
    File::open(path)?.take(131073).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 131072 && raw(&bytes) == receipt.manifest_digest,
        "base manifest receipt mismatch"
    );
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn prepare(request: ExperimentRequest<'_>) -> Result<PreparedLexical> {
    let started = Instant::now();
    let input = changed_input(
        request.base_inventory,
        request.inventory,
        request.base_receipt.token,
        request.target,
    )?;
    ensure!(
        input.full_rebuild_reasons.is_empty(),
        "lexical experiment requires full rebuild: {:?}",
        input.full_rebuild_reasons
    );
    let old_bytes = request.base_inventory.canonical_bytes()?;
    let bytes = request.inventory.canonical_bytes()?;
    ensure!(
        raw(&old_bytes) == request.base_receipt.inventory_digest
            && revision_digest(
                request.base_inventory.retrieval_specification.as_bytes(),
                &old_bytes
            ) == request.base_receipt.token.revision
            && revision_digest(request.inventory.retrieval_specification.as_bytes(), &bytes)
                == request.target.revision,
        "lexical inventory token mismatch"
    );
    request.lineage.validate()?;
    ensure!(
        request.lineage.incarnation == request.target.incarnation
            && request.lineage.observation.generation == request.target.generation
            && request.lineage.observation.parent == Some(request.base_receipt.token)
            && raw(&serde_json::to_vec(request.lineage)?) == request.inventory.lineage_digest,
        "lexical target lineage mismatch"
    );
    let _pin = PersistentIndex::open_v2(
        request.collection_directory,
        &request.inventory.collection,
        request.base_receipt,
    )?;
    let base = request
        .collection_directory
        .join("generations")
        .join(request.base_receipt.token.revision.to_string());
    let manifest = read_manifest(&base.join("manifest.json"), request.base_receipt)?;
    ensure!(
        manifest.vectors == 0 && manifest.model_sha256.is_none(),
        "vector generation is not eligible"
    );
    for path in request.staging_parent.ancestors() {
        ensure!(
            !path.symlink_metadata()?.file_type().is_symlink(),
            "linked lexical staging parent"
        );
    }
    let staging_parent = request.staging_parent.canonicalize()?;
    let collection = request.collection_directory.canonicalize()?;
    ensure!(
        !staging_parent.starts_with(collection.join("generations"))
            && !staging_parent.starts_with(collection.join("objects"))
            && !staging_parent.starts_with(collection.join("history")),
        "lexical staging cannot mutate a retained representation"
    );
    let directory = tempfile::Builder::new()
        .prefix("airk-lexical-experiment-")
        .tempdir_in(request.staging_parent)?;
    let charged = Arc::new(AtomicU64::new(0));
    let counter = charged.clone();
    let charge: Charge = Arc::new(move |bytes| {
        (request.charge)(bytes)?;
        counter.fetch_add(bytes, Ordering::Relaxed);
        Ok(())
    });
    let mut copied_bytes = 0u64;
    for (path, component) in &manifest.components {
        let Some(name) = path.strip_prefix("lexical/") else {
            continue;
        };
        ensure!(
            Path::new(name).components().count() == 1
                && Path::new(name)
                    .components()
                    .all(|part| matches!(part, Component::Normal(_))),
            "invalid lexical component name"
        );
        let source = base.join(path);
        let metadata = source.symlink_metadata()?;
        ensure!(
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() == component.bytes,
            "invalid base lexical file"
        );
        let mut from = File::open(&source)?;
        let mut to = BudgetWriter::new(File::create(directory.path().join(name))?, charge.clone());
        let mut hash = Sha256::new();
        let mut remaining = component.bytes;
        let mut buffer = [0u8; 65536];
        while remaining > 0 {
            let size = remaining.min(buffer.len() as u64) as usize;
            from.read_exact(&mut buffer[..size])?;
            hash.update(&buffer[..size]);
            to.write_all(&buffer[..size])?;
            remaining -= size as u64;
        }
        ensure!(
            from.metadata()?.len() == component.bytes
                && Sha256Digest::from_bytes(hash.finalize().into()) == component.sha256,
            "base lexical bytes changed while copying"
        );
        to.get_ref().sync_all()?;
        copied_bytes = copied_bytes
            .checked_add(component.bytes)
            .context("copy size overflow")?;
    }
    let index = Index::open(BoundedDirectory::new(
        tantivy::directory::MmapDirectory::open(directory.path())?,
        charge,
    ))?;
    let schema = index.schema();
    ensure!(
        schema == expected_schema(),
        "unsupported lexical experiment schema"
    );
    let path_field = schema.get_field("path")?;
    let terms_field = schema.get_field("terms")?;
    let ordinal_field = schema.get_field("ordinal")?;
    let payload_field = schema.get_field("payload")?;
    let mut writer = index.writer_with_num_threads::<TantivyDocument>(1, 20_000_000)?;
    let reload: BTreeSet<_> = input
        .added
        .iter()
        .chain(&input.replaced)
        .chain(&input.ordinal_shifted)
        .collect();
    for path in input.removed.iter().chain(reload.iter().copied()) {
        writer.delete_term(Term::from_field_text(path_field, path));
    }
    let ordinals = starts(request.inventory)?;
    for path in &reload {
        let artifact = request.source.load_artifact(path)?;
        let file = &request.inventory.files[*path];
        ensure!(
            artifact.path == **path
                && artifact.content_hash == file.state.source_hash.to_string()
                && artifact.chunks.len() == file.chunks,
            "changed lexical artifact binding mismatch"
        );
        for (offset, chunk) in artifact.chunks.iter().enumerate() {
            let mut document = TantivyDocument::default();
            for term in terms(&format!("{path} {}", chunk.text)) {
                document.add_text(terms_field, term);
            }
            document.add_text(path_field, path);
            document.add_u64(ordinal_field, ordinals[*path] + offset as u64);
            document.add_text(
                payload_field,
                serde_json::to_string(&Hit {
                    citation: Citation {
                        path: (*path).clone(),
                        content_hash: artifact.content_hash.clone(),
                        start_line: chunk.start_line,
                        end_line: chunk.end_line,
                        location: chunk.location.clone(),
                    },
                    text: chunk.text.clone(),
                    truncated: false,
                })?,
            );
            writer.add_document(document)?;
        }
    }
    writer.commit()?;
    writer.wait_merging_threads()?;
    drop(index);
    let index = Index::open_in_dir(directory.path())?;
    let reader: IndexReader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::Manual)
        .try_into()?;
    let count = request
        .inventory
        .files
        .values()
        .map(|file| file.chunks as u64)
        .sum::<u64>();
    ensure!(
        reader.searcher().num_docs() == count,
        "incremental lexical count mismatch"
    );
    let mut output_bytes = 0u64;
    for (ordinal, entry) in fs::read_dir(directory.path())?.enumerate() {
        ensure!(ordinal < 1024, "lexical output component count exceeded");
        let entry = entry?;
        let metadata = entry.path().symlink_metadata()?;
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "invalid lexical experiment output"
        );
        output_bytes = output_bytes
            .checked_add(metadata.len())
            .context("output size overflow")?;
    }
    let loaded_artifacts = reload.len();
    Ok(PreparedLexical {
        reader,
        directory,
        target: request.target,
        collection: request.inventory.collection.clone(),
        metrics: ExperimentMetrics {
            input,
            elapsed_micros: started.elapsed().as_micros(),
            copied_bytes,
            charged_bytes: charged.load(Ordering::Relaxed),
            output_bytes,
            loaded_artifacts,
        },
    })
}

impl PreparedLexical {
    pub fn directory(&self) -> &Path {
        self.directory.path()
    }
    /// Same term/path and global-ordinal query contract as the production reader.
    pub fn search(&self, query: &SearchQuery) -> Result<SearchResponse> {
        query.validate()?;
        let searcher = self.reader.searcher();
        let schema = searcher.schema();
        let terms = terms(&query.text);
        ensure!(!terms.is_empty(), "query must contain a letter or number");
        let field = schema.get_field("terms")?;
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = terms
            .into_iter()
            .map(|term| {
                (
                    Occur::Must,
                    Box::new(TermQuery::new(
                        Term::from_field_text(field, &term),
                        IndexRecordOption::Basic,
                    )) as Box<dyn Query>,
                )
            })
            .collect();
        if let Some(prefix) = &query.path_prefix {
            let escaped: String = prefix
                .chars()
                .flat_map(|ch| {
                    if ".+*?()|[]{}^$\\".contains(ch) {
                        vec!['\\', ch]
                    } else {
                        vec![ch]
                    }
                })
                .collect();
            clauses.push((
                Occur::Must,
                Box::new(RegexQuery::from_pattern(
                    &format!("(?s:{escaped}.*)"),
                    schema.get_field("path")?,
                )?),
            ));
        }
        let candidates = searcher.search(
            &BooleanQuery::new(clauses),
            &TopDocs::with_limit(query.limit + 1).order_by_u64_field("ordinal", Order::Asc),
        )?;
        let mut response = SearchResponse {
            publication: Some(airk_core::evidence::RevisionEnvelope::new(
                self.collection.clone(),
                self.target,
            )),
            semantic_readiness: None,
            discovery_readiness: None,
            generation: self.target.generation,
            collection: self.collection.clone(),
            hits: vec![],
            truncated: false,
        };
        let mut remaining = query.max_bytes;
        for (_, address) in candidates {
            if remaining == 0 || response.hits.len() == query.limit {
                response.truncated = true;
                break;
            }
            let document: TantivyDocument = searcher.doc(address)?;
            let mut hit: Hit = serde_json::from_str(
                document
                    .get_first(schema.get_field("payload")?)
                    .and_then(|value| value.as_str())
                    .context("missing lexical payload")?,
            )?;
            let (text, truncated) = truncate_utf8(&hit.text, remaining);
            hit.text = text;
            hit.truncated |= truncated;
            remaining -= hit.text.len();
            response.truncated |= hit.truncated;
            response.hits.push(hit);
        }
        Ok(response)
    }
}
