// SPDX-License-Identifier: Apache-2.0
//! Immutable Tantivy/DiskANN components, selected by the complete revision digest.
use crate::{
    bounded::{BoundedDirectory, BudgetWriter, Charge},
    model::{EmbeddingCacheContext, EmbeddingModel},
    terms,
    vector::DiskIndex,
};
use airk_core::identity::{PublicationReceipt, RevisionToken, Sha256Digest, revision_digest};
use airk_core::model_objects::{MAX_MODEL_OBJECT_SET_BYTES, ModelObjectSet};
use airk_core::publication::InventoryV2;
use airk_core::vector_selection::{VectorNodeRegistry, VectorSelectionPolicy};
use airk_core::{Citation, Hit, RetrievalSource, Snapshot};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::Mutex,
};
use tantivy::{
    Index, IndexReader, ReloadPolicy, TantivyDocument,
    schema::{FAST, Field, INDEXED, STORED, STRING, Schema},
};

#[cfg(test)]
#[path = "../tests/support/source.rs"]
mod fixture_source;
mod search;
mod vector_inputs;
mod vector_map;

const ENGINE: &str = "tantivy-0.26.2/diskann-0.59.0/airk-file-1/generation-7/inventory-4/model-objects-1/source-context-1/vector-units-1/vector-map-1/vector-selection-1";
const MAX_MANIFEST_V2_BYTES: usize = 128 * 1024;
const MAX_COMPONENTS_V2: usize = 1024;
const MAX_INVENTORY_V2_BYTES: usize = airk_core::publication::MAX_INVENTORY_BYTES;

pub fn retrieval_specification() -> &'static [u8] {
    ENGINE.as_bytes()
}

/// Kernel-finalized identity plus the bound artifact projection needed to build
/// retrieval components. Inventory bytes are preserved verbatim, never reencoded.
pub struct V2Preparation<'a> {
    pub snapshot: &'a Snapshot,
    pub canonical_inventory: &'a [u8],
    pub token: RevisionToken,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentDigest {
    pub bytes: u64,
    pub sha256: Sha256Digest,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestV2 {
    pub schema: u32,
    pub engine: String,
    pub token: RevisionToken,
    pub collection: String,
    pub chunks: usize,
    pub vectors: usize,
    pub vector_occurrences: usize,
    pub vector_selection_digest: Sha256Digest,
    pub vector_eligible_artifacts: usize,
    pub vector_excluded_artifacts: usize,
    pub model_sha256: Option<String>,
    pub embedding: String,
    pub inventory_digest: Sha256Digest,
    pub components: BTreeMap<String, ComponentDigest>,
}

struct BuiltComponents {
    chunks: usize,
    vectors: usize,
    vector_occurrences: usize,
    vector_eligible_artifacts: usize,
    vector_excluded_artifacts: usize,
    model_sha256: Option<String>,
    embedding: &'static str,
}

fn schema() -> (Schema, Fields) {
    let mut builder = Schema::builder();
    let fields = Fields {
        terms: builder.add_text_field("terms", STRING),
        path: builder.add_text_field("path", STRING),
        ordinal: builder.add_u64_field("ordinal", FAST | INDEXED),
        payload: builder.add_text_field("payload", STORED),
    };
    (builder.build(), fields)
}

fn reader_lease_path(base: &Path, revision: &str) -> Result<PathBuf> {
    let root = base
        .parent()
        .context("collection parent missing")?
        .join(".reader-locks");
    let collection = root.join(base.file_name().context("collection name missing")?);
    let lease = collection.join(revision);
    for path in [&root, &collection, &lease] {
        if let Ok(metadata) = fs::symlink_metadata(path) {
            ensure!(
                !metadata.file_type().is_symlink(),
                "linked generation lease state"
            );
        }
    }
    Ok(lease)
}

struct Fields {
    terms: Field,
    path: Field,
    ordinal: Field,
    payload: Field,
}

#[allow(clippy::too_many_arguments)]
fn build_streaming_components(
    directory: &Path,
    snapshot: &Snapshot,
    model_directory: Option<&Path>,
    source: &dyn RetrievalSource,
    registry: Option<&dyn VectorNodeRegistry>,
    selection: &VectorSelectionPolicy,
    charge: Charge,
    mut cache: Option<&mut EmbeddingCacheContext<'_>>,
    expected_pipeline: Option<Sha256Digest>,
    model_objects: Option<(&Path, &ModelObjectSet)>,
) -> Result<BuiltComponents> {
    let selection = selection.validated()?;
    let lexical = directory.join("lexical");
    fs::create_dir(&lexical)?;
    let (schema, fields) = schema();
    let index = Index::create(
        BoundedDirectory::new(
            tantivy::directory::MmapDirectory::open(&lexical)?,
            charge.clone(),
        ),
        schema,
        tantivy::IndexSettings::default(),
    )?;
    let mut writer = index.writer_with_num_threads::<TantivyDocument>(1, 20_000_000)?;
    let map_directory = directory.join("vector-map");
    fs::create_dir(&map_directory)?;
    let (map_schema, map_fields) = vector_map::schema();
    let map_index = Index::create(
        BoundedDirectory::new(
            tantivy::directory::MmapDirectory::open(&map_directory)?,
            charge.clone(),
        ),
        map_schema,
        tantivy::IndexSettings::default(),
    )?;
    let mut map_writer = map_index.writer_with_num_threads::<TantivyDocument>(1, 20_000_000)?;

    ensure!(
        model_directory.is_none() || model_objects.is_none(),
        "conflicting model inputs"
    );
    let model_started = std::time::Instant::now();
    let model = match model_objects {
        Some((collection, set)) => Some(EmbeddingModel::open_objects(collection, set)?),
        None => model_directory.map(EmbeddingModel::open).transpose()?,
    };
    let mut local_stats = crate::model::EmbeddingCacheStats {
        plan_cache_peak_bytes: model
            .as_ref()
            .map(EmbeddingModel::plan_cache_peak_bytes)
            .transpose()?
            .unwrap_or(0),
        ..Default::default()
    };
    if model.is_some()
        && let Some(context) = cache.as_mut()
    {
        context.stats.model_load_us = crate::embedding_batch::elapsed_us(model_started);
        context.stats.plan_cache_peak_bytes = context
            .stats
            .plan_cache_peak_bytes
            .max(local_stats.plan_cache_peak_bytes);
    }
    let descriptor = model
        .as_ref()
        .map(EmbeddingModel::cache_descriptor)
        .transpose()?;
    if let Some(expected) = expected_pipeline {
        ensure!(
            descriptor
                .as_ref()
                .map(|value| value.identity())
                .transpose()?
                == Some(expected),
            "model pipeline changed after inventory finalization"
        );
    }
    ensure!(
        snapshot.embedding_model.as_deref() == model.as_ref().map(EmbeddingModel::identity),
        "model identity changed while preparing components"
    );
    let mut graph = model
        .as_ref()
        .map(|model| {
            DiskIndex::create_with_budget(
                &directory.join("vectors.bin"),
                model.dimensions(),
                charge.clone(),
            )
        })
        .transpose()?;
    let mut ordinal = 0u64;
    let mut vectors = 0usize;
    let mut vector_occurrences = 0usize;
    let mut vector_eligible_artifacts = 0usize;
    let mut vector_excluded_artifacts = 0usize;
    for path in snapshot.artifacts.keys() {
        let retained = source.load_retrieval_artifact(path)?;
        let artifact = &retained.artifact;
        retained.payload.validate()?;
        let decision = selection.decision(path, &retained.profile, &artifact.language)?;
        let stats = match cache.as_mut() {
            Some(context) => &mut *context.stats,
            None => &mut local_stats,
        };
        if decision.eligible {
            vector_eligible_artifacts += 1;
            stats.vector_eligible_artifacts = stats.vector_eligible_artifacts.saturating_add(1);
        } else {
            vector_excluded_artifacts += 1;
            stats.vector_excluded_artifacts = stats.vector_excluded_artifacts.saturating_add(1);
        }
        ensure!(
            artifact.path == *path && artifact.chunks.len() == retained.payload.chunks.len(),
            "retrieval carrier differs from retained artifact"
        );
        let mut mapping_budget = airk_core::location::HostBudget::new(500_000, 10_000, 16 << 20)?;
        let mapper = (!retained.payload.regions.is_empty())
            .then(|| {
                airk_core::mapped_binding::MappedBinding::new(
                    &retained.payload,
                    &retained.profile,
                    &mut mapping_budget,
                )
            })
            .transpose()?;
        let first_ordinal = ordinal;
        for (chunk, local) in artifact.chunks.iter().zip(&retained.payload.chunks) {
            let text = match local.region {
                None => retained.payload.text.as_str(),
                Some(region) => mapper
                    .as_ref()
                    .context("region mapper missing")?
                    .text(Some(region))?,
            };
            ensure!(
                text.get(local.span.start..local.span.end) == Some(chunk.text.as_str()),
                "bound chunk differs from neutral retrieval payload"
            );
            let mut document = TantivyDocument::default();
            for term in terms(&format!("{path} {}", chunk.text)) {
                document.add_text(fields.terms, term);
            }
            document.add_text(fields.path, path);
            document.add_u64(fields.ordinal, ordinal);
            let hit = Hit {
                citation: Citation {
                    path: path.clone(),
                    content_hash: artifact.content_hash.clone(),
                    start_line: chunk.start_line,
                    end_line: chunk.end_line,
                    location: chunk.location.clone(),
                },
                text: chunk.text.clone(),
                truncated: false,
            };
            document.add_text(fields.payload, serde_json::to_string(&hit)?);
            writer.add_document(document)?;
            ordinal = ordinal.checked_add(1).context("lexical ordinal overflow")?;
        }
        if let Some(model) = &model {
            if !decision.eligible {
                continue;
            }
            let registry =
                registry.context("model build requires a private exact-input registry")?;
            let descriptor = descriptor
                .as_ref()
                .context("embedding descriptor missing")?;
            let mut units = Vec::with_capacity(crate::embedding_batch::MAX_BATCH_CHUNKS);
            let mut flush =
                |units: &mut Vec<crate::vector_units::PreparedVectorUnit>| -> Result<()> {
                    if units.is_empty() {
                        return Ok(());
                    }
                    let texts: Vec<_> = units.iter().map(|unit| unit.input.as_str()).collect();
                    let ids = vector_inputs::intern(
                        model,
                        &texts,
                        descriptor,
                        registry,
                        graph.as_mut().context("vector graph missing")?,
                        cache.as_deref_mut(),
                        &mut local_stats,
                    )?;
                    vectors = graph.as_ref().context("vector graph missing")?.len();
                    for (unit, graph_id) in units.drain(..).zip(ids) {
                        let Some(graph_id) = graph_id else {
                            continue;
                        };
                        ensure!(
                            !unit.members.is_empty()
                                && unit.members.len() <= vector_map::MAX_MEMBERS,
                            "invalid vector unit member count"
                        );
                        let mut members = Vec::with_capacity(unit.members.len());
                        for member in unit.members {
                            let index = member.chunk_index as usize;
                            let local = retained
                                .payload
                                .chunks
                                .get(index)
                                .context("vector member chunk missing")?;
                            let chunk = &artifact.chunks[index];
                            ensure!(
                                local.span.start <= member.span.start
                                    && member.span.end <= local.span.end
                                    && member.span.start < member.span.end,
                                "vector member outside lexical chunk"
                            );
                            let span_in_chunk = airk_core::location::ByteSpan {
                                start: member.span.start - local.span.start,
                                end: member.span.end - local.span.start,
                            };
                            span_in_chunk.validate(&chunk.text)?;
                            let (location, start_line, end_line) =
                                if let Some(region) = local.region {
                                    mapper
                                        .as_ref()
                                        .context("vector region mapper missing")?
                                        .mapped_location(
                                            region,
                                            member.span.clone(),
                                            &mut mapping_budget,
                                        )?
                                } else {
                                    let start_line = chunk.start_line
                                        + chunk.text[..span_in_chunk.start]
                                            .bytes()
                                            .filter(|byte| *byte == b'\n')
                                            .count();
                                    let end_line = start_line
                                        + chunk.text.as_bytes()
                                            [span_in_chunk.start..span_in_chunk.end - 1]
                                            .iter()
                                            .filter(|byte| **byte == b'\n')
                                            .count();
                                    let location = match &chunk.location {
                                        airk_core::DocumentLocation::Lines { .. } => {
                                            airk_core::DocumentLocation::Lines {
                                                start: start_line,
                                                end: end_line,
                                            }
                                        }
                                        location => location.clone(),
                                    };
                                    (location, start_line, end_line)
                                };
                            let mut member = vector_map::Member {
                                evidence_id: Sha256Digest::from_bytes([0; 32]),
                                lexical_ordinal: first_ordinal + index as u64,
                                region: local.region,
                                span: member.span.clone(),
                                blank: chunk.text[span_in_chunk.start..span_in_chunk.end]
                                    .trim()
                                    .is_empty(),
                                span_in_chunk,
                                start_line,
                                end_line,
                                location,
                            };
                            member.set_identity(path, artifact.content_hash.parse()?)?;
                            members.push(member);
                        }
                        let first = members.first().context("vector body is empty")?;
                        let last = members.last().context("vector body is empty")?;
                        let span = airk_core::extraction::LocalSpan {
                            start: first.span.start,
                            end: last.span.end,
                        };
                        let (location, start_line, end_line, projection) = if let Some(region) =
                            first.region
                        {
                            let mapper = mapper.as_ref().context("vector body mapper missing")?;
                            let projection =
                                mapper.capture_body(region, span.clone(), &mut mapping_budget)?;
                            let (location, start, end) = mapper.mapped_location(
                                region,
                                span.clone(),
                                &mut mapping_budget,
                            )?;
                            (location, start, end, Some(projection))
                        } else {
                            let location = match &first.location {
                                airk_core::DocumentLocation::Lines { .. } => {
                                    airk_core::DocumentLocation::Lines {
                                        start: first.start_line,
                                        end: last.end_line,
                                    }
                                }
                                location => {
                                    ensure!(
                                        members.iter().all(|member| &member.location == location),
                                        "vector body crosses native document location"
                                    );
                                    location.clone()
                                }
                            };
                            (location, first.start_line, last.end_line, None)
                        };
                        let body = vector_map::Body {
                            region: first.region,
                            span,
                            start_line,
                            end_line,
                            location,
                            projection,
                        };
                        let occurrence = vector_map::Occurrence::new(
                            path.clone(),
                            retained.payload_digest,
                            artifact.content_hash.parse()?,
                            airk_core::embedding::embedding_key(descriptor, unit.input.as_bytes())?,
                            body,
                            members,
                        )?;
                        map_writer.add_document(vector_map::document(
                            &map_fields,
                            graph_id,
                            vector_occurrences as u64,
                            &occurrence,
                        )?)?;
                        vector_occurrences += 1;
                    }
                    Ok(())
                };
            let planning =
                crate::vector_units::visit_vector_units(&retained.payload, model, |unit| {
                    units.push(unit);
                    if units.len() == crate::embedding_batch::MAX_BATCH_CHUNKS {
                        flush(&mut units)?;
                    }
                    Ok(())
                })?;
            flush(&mut units)?;
            let stats = match cache.as_mut() {
                Some(context) => &mut *context.stats,
                None => &mut local_stats,
            };
            stats.vector_units = stats.vector_units.saturating_add(planning.units);
            stats.vector_body_bytes = stats.vector_body_bytes.saturating_add(planning.body_bytes);
            stats.vector_planning_us = stats
                .vector_planning_us
                .saturating_add(planning.planning_us);
        }
    }
    writer.commit()?;
    writer.wait_merging_threads()?;
    drop(index);
    map_writer.commit()?;
    map_writer.wait_merging_threads()?;
    drop(map_index);
    if let Some(mut graph) = graph {
        graph.finish()?;
        drop(graph);
        DiskIndex::open(&directory.join("vectors.bin"))?;
    }
    if let Some(model_directory) = model_directory {
        fs::create_dir(directory.join("model"))?;
        for name in model
            .as_ref()
            .context("model adapter missing")?
            .asset_names()
        {
            let destination = directory.join("model").join(name);
            let mut input = File::open(model_directory.join(name))?;
            let mut output = BudgetWriter::new(File::create(destination)?, charge.clone());
            std::io::copy(&mut input, &mut output)?;
            output.get_ref().sync_all()?;
        }
        let copied = crate::model::inspect(&directory.join("model"))?;
        ensure!(
            Some(copied.identity.as_str()) == model.as_ref().map(EmbeddingModel::identity),
            "model changed while building revision"
        );
        ensure!(
            Some(crate::model::inspect_cache_descriptor(
                &directory.join("model")
            )?) == descriptor,
            "model pipeline changed while copying revision assets"
        );
    }
    if let Some((_, set)) = model_objects {
        let mut output = BudgetWriter::new(
            File::create(directory.join("model-objects.json"))?,
            charge.clone(),
        );
        output.write_all(&set.canonical_bytes()?)?;
        output.get_ref().sync_all()?;
    }
    Ok(BuiltComponents {
        chunks: ordinal as usize,
        vectors,
        vector_occurrences,
        vector_eligible_artifacts,
        vector_excluded_artifacts,
        model_sha256: model.as_ref().map(|m| m.identity().to_owned()),
        embedding: model.as_ref().map_or("none", EmbeddingModel::specification),
    })
}

fn raw_digest(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

fn bounded_file(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "invalid component file"
    );
    ensure!(
        metadata.len() <= maximum as u64,
        "component exceeds read bound"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= maximum, "component exceeds read bound");
    Ok(bytes)
}

fn component_digests(directory: &Path) -> Result<BTreeMap<String, ComponentDigest>> {
    component_digests_expected(directory, None)
}

fn component_digests_expected(
    directory: &Path,
    expected: Option<&BTreeMap<String, ComponentDigest>>,
) -> Result<BTreeMap<String, ComponentDigest>> {
    ensure!(
        !fs::symlink_metadata(directory)?.file_type().is_symlink(),
        "linked generation directory"
    );
    let mut pending = vec![directory.to_path_buf()];
    let mut components = BTreeMap::new();
    while let Some(parent) = pending.pop() {
        for entry in fs::read_dir(parent)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path.strip_prefix(directory)?;
            ensure!(
                relative
                    .components()
                    .all(|part| matches!(part, Component::Normal(_))),
                "unsafe component path"
            );
            let name = relative
                .to_str()
                .context("non-UTF8 component path")?
                .replace('\\', "/");
            ensure!(name.len() <= 4096, "oversized component path");
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(
                !metadata.file_type().is_symlink(),
                "linked generation component"
            );
            if metadata.is_dir() {
                ensure!(
                    name == "lexical" || name == "vector-map",
                    "foreign generation directory"
                );
                pending.push(path);
                continue;
            }
            ensure!(metadata.is_file(), "non-file generation component");
            if name == "manifest.json" {
                continue;
            }
            ensure!(
                matches!(
                    name.as_str(),
                    "inventory.json"
                        | "lineage.json"
                        | "catalog.sqlite"
                        | "vectors.bin"
                        | "embedding-cache.json"
                        | "model-objects.json"
                ) || name.starts_with("lexical/")
                    || name.starts_with("vector-map/"),
                "foreign generation component"
            );
            ensure!(
                components.len() < MAX_COMPONENTS_V2,
                "too many generation components"
            );
            if let Some(expected) = expected {
                ensure!(
                    expected
                        .get(&name)
                        .is_some_and(|value| value.bytes == metadata.len()),
                    "unexpected component or changed component size"
                );
            }
            let mut file = File::open(&path)?;
            let mut hash = Sha256::new();
            let mut buffer = [0u8; 64 * 1024];
            let mut bytes = 0u64;
            // Empty Tantivy coordination files may be locked by another reader.
            // Windows rejects even an EOF read through an exclusive byte lock;
            // their checked length determines SHA256(empty) without that read.
            while bytes < metadata.len() {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                bytes = bytes
                    .checked_add(read as u64)
                    .context("component size overflow")?;
                ensure!(
                    bytes <= metadata.len(),
                    "component grew during verification"
                );
                hash.update(&buffer[..read]);
            }
            ensure!(
                bytes == metadata.len() && file.metadata()?.len() == metadata.len(),
                "component changed during verification"
            );
            components.insert(
                name,
                ComponentDigest {
                    bytes,
                    sha256: Sha256Digest::from_bytes(hash.finalize().into()),
                },
            );
        }
    }
    Ok(components)
}

fn sync_components(directory: &Path, components: &BTreeMap<String, ComponentDigest>) -> Result<()> {
    for name in components.keys() {
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(directory.join(name))?
            .sync_all()?;
    }
    // Tantivy's directory implementation supplies its platform-specific sync
    // behavior. Windows directory durability is not a power-loss guarantee.
    for name in ["lexical", "vector-map", ""] {
        let path = directory.join(name);
        if path.is_dir() {
            tantivy::directory::Directory::sync_directory(
                &tantivy::directory::MmapDirectory::open(path)?,
            )?;
        }
    }
    Ok(())
}

fn read_manifest_v2(
    directory: &Path,
    receipt: &PublicationReceipt,
    collection: &str,
) -> Result<ManifestV2> {
    let bytes = bounded_file(&directory.join("manifest.json"), MAX_MANIFEST_V2_BYTES)?;
    ensure!(
        raw_digest(&bytes) == receipt.manifest_digest,
        "v2 manifest receipt mismatch"
    );
    let header: serde_json::Value = serde_json::from_slice(&bytes)?;
    ensure!(
        header.get("schema").and_then(serde_json::Value::as_u64) == Some(4)
            && header.get("engine").and_then(serde_json::Value::as_str) == Some(ENGINE),
        "obsolete retrieval format; rebuild the index with airk index --full"
    );
    let manifest: ManifestV2 = serde_json::from_slice(&bytes)?;
    ensure!(
        (manifest.schema == 4 && manifest.engine == ENGINE)
            && manifest.token == receipt.token
            && manifest.collection == collection
            && manifest.inventory_digest == receipt.inventory_digest
            && manifest.components.len() <= MAX_COMPONENTS_V2,
        "v2 manifest identity mismatch"
    );
    for name in manifest.components.keys() {
        ensure!(
            !name.starts_with("model/"),
            "model storage disagrees with generation schema"
        );
        ensure!(
            name.len() <= 4096
                && !name.contains('\\')
                && Path::new(name)
                    .components()
                    .all(|part| matches!(part, Component::Normal(_))),
            "invalid manifest component path"
        );
    }
    for name in [
        "inventory.json",
        "lineage.json",
        "catalog.sqlite",
        "lexical/meta.json",
        "vector-map/meta.json",
    ] {
        ensure!(
            manifest.components.contains_key(name),
            "required v2 component missing: {name}"
        );
    }
    ensure!(
        manifest.components["inventory.json"].sha256 == receipt.inventory_digest,
        "inventory receipt mismatch"
    );
    Ok(manifest)
}

fn verify_directory_v2(
    directory: &Path,
    receipt: &PublicationReceipt,
    collection: &str,
) -> Result<ManifestV2> {
    let manifest = read_manifest_v2(directory, receipt, collection)?;
    ensure!(
        component_digests_expected(directory, Some(&manifest.components))? == manifest.components,
        "v2 generation component checksum mismatch"
    );
    let inventory = bounded_file(&directory.join("inventory.json"), MAX_INVENTORY_V2_BYTES)?;
    ensure!(
        revision_digest(manifest.engine.as_bytes(), &inventory) == receipt.token.revision,
        "v2 revision input mismatch"
    );
    {
        ensure!(
            !directory.join("model").exists(),
            "shared-model generation contains inline model directory"
        );
        let objects = read_model_objects(directory, &manifest)?;
        let inventory: InventoryV2 = serde_json::from_slice(&inventory)?;
        let selection = inventory.vector_selection.validated()?;
        let mut eligible = 0usize;
        for (path, file) in &inventory.files {
            let profile = inventory
                .profiles
                .get(&file.state.profile_id)
                .context("vector policy profile missing")?;
            eligible += usize::from(selection.decision(path, profile, &file.language)?.eligible);
        }
        ensure!(
            manifest.vector_selection_digest == inventory.vector_selection.digest()?
                && manifest.vector_eligible_artifacts == eligible
                && manifest.vector_excluded_artifacts == inventory.files.len() - eligible,
            "vector selection manifest differs from authoritative inventory"
        );
        ensure!(
            objects.as_ref().map(|set| set.pipeline_digest) == inventory.embedding_pipeline_digest,
            "model object pipeline differs from authoritative inventory"
        );
    }
    Ok(manifest)
}

fn read_model_objects(directory: &Path, manifest: &ManifestV2) -> Result<Option<ModelObjectSet>> {
    match &manifest.model_sha256 {
        None => {
            ensure!(
                !manifest.components.contains_key("model-objects.json")
                    && !directory.join("model-objects.json").exists(),
                "lexical-only generation contains model references"
            );
            Ok(None)
        }
        Some(identity) => {
            let expected = manifest
                .components
                .get("model-objects.json")
                .context("model object sidecar missing")?;
            let bytes = bounded_file(
                &directory.join("model-objects.json"),
                MAX_MODEL_OBJECT_SET_BYTES,
            )?;
            ensure!(
                bytes.len() as u64 == expected.bytes && raw_digest(&bytes) == expected.sha256,
                "model object sidecar checksum mismatch"
            );
            let set = ModelObjectSet::from_bytes(&bytes)?;
            ensure!(
                set.legacy_identity == *identity,
                "model object display identity mismatch"
            );
            Ok(Some(set))
        }
    }
}

fn open_components_v2(
    directory: &Path,
    collection: &Path,
    manifest: &ManifestV2,
) -> Result<(IndexReader, Fields)> {
    let (expected, fields) = schema();
    let index = Index::open_in_dir(directory.join("lexical"))?;
    ensure!(index.schema() == expected, "unsupported lexical schema");
    let reader: IndexReader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::Manual)
        .try_into()?;
    ensure!(
        reader.searcher().num_docs() == manifest.chunks as u64,
        "lexical inventory mismatch"
    );
    ensure!(
        manifest.vectors <= manifest.vector_occurrences
            && (manifest.vectors == 0) == (manifest.vector_occurrences == 0),
        "vector occurrence inventory mismatch"
    );
    if manifest.model_sha256.is_some() {
        let set = read_model_objects(directory, manifest)?.context("model objects missing")?;
        crate::model::inspect_objects_cache_descriptor(collection, &set)?;
        ensure!(
            DiskIndex::open(&directory.join("vectors.bin"))?.len() == manifest.vectors,
            "vector inventory mismatch"
        );
    } else {
        ensure!(
            manifest.vectors == 0
                && manifest.vector_occurrences == 0
                && !directory.join("vectors.bin").exists()
                && !directory.join("model").exists(),
            "model-free revision contains vector/model state"
        );
        ensure!(
            !directory.join("model-objects.json").exists(),
            "model-free generation contains model references"
        );
    }
    let (map_reader, _) = vector_map::open(directory, manifest.vector_occurrences)?;
    drop(map_reader);
    Ok((reader, fields))
}

/// Build one privately finalized revision; publication remains the caller's responsibility.
#[allow(clippy::too_many_arguments)]
pub fn prepare_streaming(
    base: &Path,
    preparation: &V2Preparation<'_>,
    model_objects: Option<&ModelObjectSet>,
    source: &dyn RetrievalSource,
    registry: Option<&dyn VectorNodeRegistry>,
    charge: Option<Charge>,
    staging_parent: Option<&Path>,
    cache: Option<&mut EmbeddingCacheContext<'_>>,
    prepare_catalog: impl FnOnce(&Path) -> Result<()>,
    before_publish: impl FnOnce(&PublicationReceipt) -> Result<()>,
) -> Result<PublicationReceipt> {
    let snapshot = preparation.snapshot;
    ensure!(
        preparation.canonical_inventory.len() <= MAX_INVENTORY_V2_BYTES,
        "oversized v2 inventory"
    );
    let authoritative: InventoryV2 = serde_json::from_slice(preparation.canonical_inventory)?;
    let engine = ENGINE;
    ensure!(
        model_objects.is_none() || registry.is_some(),
        "model build requires a private exact-input registry"
    );
    if let Some(set) = model_objects {
        set.validate()?;
    }
    ensure!(
        model_objects.map(|set| set.pipeline_digest) == authoritative.embedding_pipeline_digest
            && model_objects.map(|set| set.legacy_identity.as_str())
                == authoritative.embedding_model.as_deref(),
        "model object input differs from finalized inventory"
    );
    ensure!(
        authoritative.canonical_bytes()? == preparation.canonical_inventory,
        "noncanonical v2 inventory bytes"
    );
    ensure!(
        authoritative.incarnation == preparation.token.incarnation
            && authoritative.retrieval_specification == engine,
        "v2 inventory identity/specification mismatch"
    );
    ensure!(
        serde_json::to_vec(&authoritative.projection()?)? == serde_json::to_vec(snapshot)?,
        "v2 inventory projection mismatch"
    );
    ensure!(
        preparation.token.generation == snapshot.generation,
        "v2 generation projection mismatch"
    );
    ensure!(
        preparation.token.revision
            == revision_digest(engine.as_bytes(), preparation.canonical_inventory),
        "v2 revision input mismatch"
    );
    let generations = base.join("generations");
    fs::create_dir_all(&generations)?;
    ensure!(
        !fs::symlink_metadata(&generations)?.file_type().is_symlink(),
        "linked generation root"
    );
    let target = generations.join(preparation.token.revision.to_string());
    ensure!(
        !target.try_exists()?,
        "v2 revision already exists; verify a trusted receipt before reuse"
    );
    let staging = tempfile::Builder::new()
        .prefix("building-v2-")
        .tempdir_in(staging_parent.unwrap_or(&generations))?;
    let charge = charge.unwrap_or_else(|| std::sync::Arc::new(|_| Ok(())));
    let built = build_streaming_components(
        staging.path(),
        snapshot,
        None,
        source,
        registry,
        &authoritative.vector_selection,
        charge.clone(),
        cache,
        authoritative.embedding_pipeline_digest,
        model_objects.map(|set| (base, set)),
    )?;
    let mut inventory = BudgetWriter::new(
        File::create(staging.path().join("inventory.json"))?,
        charge.clone(),
    );
    inventory.write_all(preparation.canonical_inventory)?;
    inventory.get_ref().sync_all()?;
    drop(inventory);
    let before_catalog = component_digests(staging.path())?;
    prepare_catalog(staging.path())?;
    ensure!(
        !staging.path().join("manifest.json").exists(),
        "catalog callback created the final manifest"
    );
    let components = component_digests(staging.path())?;
    ensure!(
        components.len()
            == before_catalog.len()
                + 2
                + usize::from(components.contains_key("embedding-cache.json"))
            && components.contains_key("catalog.sqlite")
            && components.contains_key("lineage.json")
            && before_catalog
                .iter()
                .all(|(name, digest)| components.get(name) == Some(digest)),
        "catalog callback altered retrieval components or omitted required files"
    );
    let inventory_digest = raw_digest(preparation.canonical_inventory);
    ensure!(
        components["lineage.json"].sha256 == authoritative.lineage_digest,
        "lineage input digest mismatch"
    );
    let expected_chunks = authoritative
        .files
        .values()
        .try_fold(0usize, |count, file| {
            count
                .checked_add(file.chunks)
                .context("chunk count overflow")
        })?;
    ensure!(
        built.chunks == expected_chunks,
        "streamed chunk inventory mismatch"
    );
    let manifest = ManifestV2 {
        schema: 4,
        engine: engine.into(),
        token: preparation.token,
        collection: snapshot.collection.clone(),
        chunks: built.chunks,
        vectors: built.vectors,
        vector_occurrences: built.vector_occurrences,
        vector_selection_digest: authoritative.vector_selection.digest()?,
        vector_eligible_artifacts: built.vector_eligible_artifacts,
        vector_excluded_artifacts: built.vector_excluded_artifacts,
        model_sha256: built.model_sha256,
        embedding: built.embedding.into(),
        inventory_digest,
        components,
    };
    let bytes = serde_json::to_vec(&manifest)?;
    ensure!(
        bytes.len() <= MAX_MANIFEST_V2_BYTES,
        "oversized v2 manifest"
    );
    let mut output = BudgetWriter::new(File::create(staging.path().join("manifest.json"))?, charge);
    output.write_all(&bytes)?;
    output.get_ref().sync_all()?;
    drop(output);
    sync_components(staging.path(), &manifest.components)?;
    let receipt = PublicationReceipt {
        token: preparation.token,
        inventory_digest,
        manifest_digest: raw_digest(&bytes),
    };
    let verified = verify_directory_v2(staging.path(), &receipt, &snapshot.collection)?;
    let (probe, _) = open_components_v2(staging.path(), base, &verified)?;
    drop(probe);
    // Reopening must not introduce an unbound file before directory publication.
    verify_directory_v2(staging.path(), &receipt, &snapshot.collection)?;
    let lease_path = reader_lease_path(base, &preparation.token.revision.to_string())?;
    fs::create_dir_all(lease_path.parent().context("lease parent missing")?)?;
    let lease = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(lease_path)?;
    lease.sync_all()?;
    drop(lease);
    before_publish(&receipt)?;
    fs::rename(staging.path(), &target).context("publish immutable v2 components")?;
    tantivy::directory::Directory::sync_directory(&tantivy::directory::MmapDirectory::open(
        &generations,
    )?)?;
    Ok(receipt)
}

pub struct PersistentIndex {
    model: Mutex<Option<EmbeddingModel>>,
    _lease: File,
    reader: IndexReader,
    fields: Fields,
    map_reader: IndexReader,
    map_fields: vector_map::Fields,
    directory: PathBuf,
    generation: u64,
    collection: String,
    pub vectors: usize,
    pub vector_occurrences: usize,
    pub vector_selection_digest: Sha256Digest,
    pub vector_eligible_artifacts: usize,
    pub vector_excluded_artifacts: usize,
    pub model_sha256: Option<String>,
    v2_receipt: Option<PublicationReceipt>,
    model_objects: Option<ModelObjectSet>,
    collection_directory: PathBuf,
}

impl PersistentIndex {
    pub fn open_v2(base: &Path, collection: &str, receipt: &PublicationReceipt) -> Result<Self> {
        let revision = receipt.token.revision.to_string();
        let directory = base.join("generations").join(&revision);
        let lease = File::open(reader_lease_path(base, &revision)?)?;
        lease.lock_shared()?;
        let manifest = verify_directory_v2(&directory, receipt, collection)?;
        let (reader, fields) = open_components_v2(&directory, base, &manifest)?;
        let model_objects = read_model_objects(&directory, &manifest)?;
        let (map_reader, map_fields) = vector_map::open(&directory, manifest.vector_occurrences)?;
        Ok(Self {
            model: Mutex::new(None),
            _lease: lease,
            reader,
            fields,
            map_reader,
            map_fields,
            directory,
            generation: receipt.token.generation,
            collection: collection.into(),
            vectors: manifest.vectors,
            vector_occurrences: manifest.vector_occurrences,
            vector_selection_digest: manifest.vector_selection_digest,
            vector_eligible_artifacts: manifest.vector_eligible_artifacts,
            vector_excluded_artifacts: manifest.vector_excluded_artifacts,
            model_sha256: manifest.model_sha256,
            v2_receipt: Some(receipt.clone()),
            model_objects,
            collection_directory: base.into(),
        })
    }
}

#[cfg(test)]
mod pipeline_tests {
    use super::*;
    use airk_core::{Artifact, Chunk, DocumentLocation};
    use std::path::PathBuf;

    struct ChangingSource {
        snapshot: Snapshot,
        model: PathBuf,
        change: bool,
    }
    impl RetrievalSource for ChangingSource {
        fn load_retrieval_artifact(&self, path: &str) -> Result<airk_core::RetrievalArtifact> {
            if self.change {
                let manifest = self.model.join("model.json");
                let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&manifest)?)?;
                value["name"] = "changed while building".into();
                fs::write(manifest, serde_json::to_vec(&value)?)?;
            }
            fixture_source::FixtureSource::new(&self.snapshot)?.load_retrieval_artifact(path)
        }
    }

    #[test]
    fn prepared_pipeline_must_match_inventory_and_copied_assets() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let model = temporary.path().join("model");
        fs::create_dir(&model)?;
        let vocabulary = br#"{"original":[1.0,0.0]}"#;
        fs::write(model.join("vocabulary.json"), vocabulary)?;
        fs::write(
            model.join("model.json"),
            serde_json::to_vec(
                &serde_json::json!({"schema":1,"name":"original race fixture","license":"Apache-2.0","dimensions":2,"sha256":format!("{:x}",Sha256::digest(vocabulary))}),
            )?,
        )?;
        let expected = crate::model::inspect_cache_descriptor(&model)?.identity()?;
        let mut snapshot = Snapshot::empty("original pipeline fixture".into());
        snapshot.embedding_model = Some(crate::model::inspect(&model)?.identity);
        snapshot.artifacts.insert(
            "original.txt".into(),
            Artifact {
                path: "original.txt".into(),
                content_hash: format!("{:x}", Sha256::digest(b"original")),
                bytes: 8,
                chunks: vec![Chunk {
                    start_line: 1,
                    end_line: 1,
                    text: "original".into(),
                    location: DocumentLocation::Lines { start: 1, end: 1 },
                }],
                language: "text".into(),
                extraction_method: "utf8".into(),
                parse_status: "complete".into(),
                facts: vec![],
                retained: None,
            },
        );
        let charge: Charge = std::sync::Arc::new(|_| Ok(()));
        let wrong = temporary.path().join("wrong");
        fs::create_dir(&wrong)?;
        let error = build_streaming_components(
            &wrong,
            &snapshot,
            Some(&model),
            &fixture_source::FixtureSource::new(&snapshot)?,
            Some(&fixture_source::FixtureRegistry::default()),
            &VectorSelectionPolicy::default(),
            charge.clone(),
            None,
            Some(Sha256Digest::from_bytes([0; 32])),
            None,
        )
        .err()
        .context("mismatched inventory pipeline accepted")?;
        assert!(error.to_string().contains("inventory finalization"));
        let legacy = temporary.path().join("legacy");
        fs::create_dir(&legacy)?;
        build_streaming_components(
            &legacy,
            &snapshot,
            Some(&model),
            &fixture_source::FixtureSource::new(&snapshot)?,
            Some(&fixture_source::FixtureRegistry::default()),
            &VectorSelectionPolicy::default(),
            charge.clone(),
            None,
            None,
            None,
        )?;
        let race = temporary.path().join("race");
        fs::create_dir(&race)?;
        let source = ChangingSource {
            snapshot: snapshot.clone(),
            model: model.clone(),
            change: true,
        };
        let error = build_streaming_components(
            &race,
            &snapshot,
            Some(&model),
            &source,
            Some(&fixture_source::FixtureRegistry::default()),
            &VectorSelectionPolicy::default(),
            charge,
            None,
            Some(expected),
            None,
        )
        .err()
        .context("model metadata race accepted")?;
        assert!(error.to_string().contains("copying revision assets"));
        assert_eq!(
            crate::model::inspect(&model)?.identity,
            snapshot.embedding_model.unwrap(),
            "legacy vocabulary identity cannot detect this race"
        );
        Ok(())
    }
}
