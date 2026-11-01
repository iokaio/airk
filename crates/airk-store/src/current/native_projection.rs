// SPDX-License-Identifier: Apache-2.0
//! Immutable native fact pages use existing retained resources, not a parallel store.
use super::{CurrentRead, resources::ResourceReference};
use crate::{
    FileStore,
    quota::{Quota, Reservation},
};
use airk_core::{Artifact, Fact, identity::Sha256Digest};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
const ROWS: usize = 256;
const TARGET_BYTES: usize = super::map::MAX_VALUE_BYTES;
pub const MAX_MANIFEST_BYTES: usize = 8 * 1024 * 1024;
const MAX_FACTS: usize = 10_000; // existing ExtractionPayload producer contract
pub const MAX_PREPARED_NATIVE_BYTES: usize = 64 << 20;
#[derive(Debug)]
pub struct ProjectionPreparationLimit;
impl std::fmt::Display for ProjectionPreparationLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("optional projection preparation exceeds bounded memory")
    }
}
impl std::error::Error for ProjectionPreparationLimit {}
pub(crate) fn bounded_encoding(value: &impl Serialize) -> Result<Vec<u8>> {
    struct Output {
        bytes: Vec<u8>,
        limited: bool,
    }
    impl std::io::Write for Output {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.bytes.len().saturating_add(bytes.len()) > MAX_PREPARED_NATIVE_BYTES {
                self.limited = true;
                return Err(std::io::Error::other("projection preparation bound"));
            }
            let required = self.bytes.len() + bytes.len();
            if self.bytes.capacity() < required {
                let capacity = self
                    .bytes
                    .capacity()
                    .max(256)
                    .saturating_mul(2)
                    .min(MAX_PREPARED_NATIVE_BYTES)
                    .max(required);
                if let Err(error) = self.bytes.try_reserve_exact(capacity - self.bytes.len()) {
                    self.limited = true;
                    return Err(std::io::Error::other(error));
                }
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut output = Output {
        bytes: Vec::new(),
        limited: false,
    };
    let result = serde_json::to_writer(&mut output, value);
    if output.limited {
        return Err(ProjectionPreparationLimit.into());
    }
    result?;
    Ok(output.bytes)
}
/// No writes or quota charges occur while constructing this sealed producer plan.
pub struct PreparedNativeProjection {
    resources: Vec<super::resources::PreparedResource>,
    root: ResourceReference,
    edges: Option<ResourceReference>,
    lines: Option<ResourceReference>,
    memory: usize,
}
impl PreparedNativeProjection {
    pub fn memory_bytes(&self) -> usize {
        self.memory
    }
}
pub struct NativeProjectionAdmission {
    pub roots: Option<Vec<ResourceReference>>,
    pub edge_roots: Vec<Option<ResourceReference>>,
    pub line_roots: Vec<Option<ResourceReference>>,
    /// Caller retains this exact guard through Applying, commit or rollback.
    pub reservation: Option<Reservation>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeFactPageRef {
    pub start: usize,
    pub rows: usize,
    pub resource: ResourceReference,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeFactHeader {
    pub version: u32,
    pub path: String,
    pub source: Sha256Digest,
    pub payload: Sha256Digest,
    pub file_metadata: Sha256Digest,
    pub profile: Sha256Digest,
    pub facts: usize,
    pub pages: Vec<NativeFactPageRef>,
    pub rank_by_ordinal: Vec<usize>,
    pub row_encoded_bytes: u64,
    pub leaf_encoded_bytes: u64,
    pub leaf_stored_bytes: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeFactRow {
    pub ordinal: usize,
    pub fact: Fact,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuity_key: Option<airk_core::continuity::ContinuityKey>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeFactPage {
    pub version: u32,
    pub page: usize,
    pub rows: Vec<NativeFactRow>,
}
#[derive(Serialize)]
struct RowRef<'a> {
    ordinal: usize,
    fact: &'a Fact,
    #[serde(skip_serializing_if = "Option::is_none")]
    continuity_key: Option<airk_core::continuity::ContinuityKey>,
}
#[derive(Serialize)]
struct PageRef<'a> {
    version: u32,
    page: usize,
    rows: Vec<RowRef<'a>>,
}
#[derive(Clone, Debug, Serialize)]
pub struct NativeProjectionStats {
    pub facts: usize,
    pub leaf_objects: usize,
    pub manifest_objects: usize,
    pub row_encoded_bytes: u64,
    pub leaf_encoded_bytes: u64,
    pub manifest_encoded_bytes: usize,
    pub leaf_stored_bytes: u64,
    pub manifest_stored_bytes: u64,
    pub retained_extraction_bytes: usize,
    pub retained_extraction_stored_bytes: u64,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct NativeProjectionBatchStats {
    pub files: u64,
    pub missing_files: u64,
    pub facts: u64,
    pub leaf_objects: u64,
    pub manifest_objects: u64,
    pub row_encoded_bytes: u64,
    pub leaf_encoded_bytes: u64,
    pub manifest_encoded_bytes: u64,
    pub retained_extraction_bytes: u64,
    pub leaf_stored_bytes: u64,
    pub manifest_stored_bytes: u64,
    pub retained_extraction_stored_bytes: u64,
}
impl NativeProjectionBatchStats {
    pub fn add(&mut self, stats: Option<&NativeProjectionStats>) -> Result<()> {
        let mut next = self.clone();
        next.files = next
            .files
            .checked_add(1)
            .context("projection file count overflow")?;
        if let Some(stats) = stats {
            next.facts = next
                .facts
                .checked_add(stats.facts as u64)
                .context("projection fact count overflow")?;
            next.leaf_objects = next
                .leaf_objects
                .checked_add(stats.leaf_objects as u64)
                .context("projection object count overflow")?;
            next.manifest_objects = next
                .manifest_objects
                .checked_add(stats.manifest_objects as u64)
                .context("projection object count overflow")?;
            next.row_encoded_bytes = next
                .row_encoded_bytes
                .checked_add(stats.row_encoded_bytes)
                .context("projection byte count overflow")?;
            next.leaf_encoded_bytes = next
                .leaf_encoded_bytes
                .checked_add(stats.leaf_encoded_bytes)
                .context("projection byte count overflow")?;
            next.manifest_encoded_bytes = next
                .manifest_encoded_bytes
                .checked_add(stats.manifest_encoded_bytes as u64)
                .context("projection byte count overflow")?;
            next.retained_extraction_bytes = next
                .retained_extraction_bytes
                .checked_add(stats.retained_extraction_bytes as u64)
                .context("projection byte count overflow")?;
            next.leaf_stored_bytes = next
                .leaf_stored_bytes
                .checked_add(stats.leaf_stored_bytes)
                .context("projection stored count overflow")?;
            next.manifest_stored_bytes = next
                .manifest_stored_bytes
                .checked_add(stats.manifest_stored_bytes)
                .context("projection stored count overflow")?;
            next.retained_extraction_stored_bytes = next
                .retained_extraction_stored_bytes
                .checked_add(stats.retained_extraction_stored_bytes)
                .context("projection stored count overflow")?;
        } else {
            next.missing_files = next
                .missing_files
                .checked_add(1)
                .context("projection missing count overflow")?;
        }
        *self = next;
        Ok(())
    }
}
#[derive(Serialize)]
pub struct NativeProjectionStatsPage {
    pub token: super::CurrentToken,
    pub counts: NativeProjectionBatchStats,
    pub next_after: Option<String>,
}

fn base(artifact: &Artifact, file_metadata: Sha256Digest) -> Result<NativeFactHeader> {
    let retained = artifact
        .retained
        .as_ref()
        .context("native projection extraction missing")?;
    ensure!(
        retained.facts <= MAX_FACTS,
        "native projection exceeds producer fact contract"
    );
    Ok(NativeFactHeader {
        version: 1,
        path: artifact.path.clone(),
        source: artifact.content_hash.parse()?,
        payload: retained.object.parse()?,
        file_metadata,
        profile: retained
            .profile
            .as_ref()
            .context("native projection profile missing")?
            .identity()?,
        facts: retained.facts,
        pages: Vec::new(),
        rank_by_ordinal: Vec::new(),
        row_encoded_bytes: 0,
        leaf_encoded_bytes: 0,
        leaf_stored_bytes: 0,
    })
}
fn order(artifact: &Artifact) -> Vec<usize> {
    let mut order = (0..artifact.facts.len()).collect::<Vec<_>>();
    order.sort_by(|a, b| {
        let left = &artifact.facts[*a];
        let right = &artifact.facts[*b];
        (left.start_line, &left.kind, &left.name, a).cmp(&(
            right.start_line,
            &right.kind,
            &right.name,
            b,
        ))
    });
    order
}
fn encode_page(
    artifact: &Artifact,
    keys: &airk_core::continuity::FactKeyIndex,
    page: usize,
    ordinals: &[usize],
) -> Result<Vec<u8>> {
    bounded_encoding(&PageRef {
        version: 1,
        page,
        rows: ordinals
            .iter()
            .map(|ordinal| RowRef {
                ordinal: *ordinal,
                fact: &artifact.facts[*ordinal],
                continuity_key: keys.key(*ordinal, &artifact.facts[*ordinal]),
            })
            .collect(),
    })
}
/// Computes actual canonical bytes with the standard serializer. A single large
/// producer-supported row is indivisible; it gets its own existing bounded resource.
fn visit_pages(
    artifact: &Artifact,
    file: &airk_core::publication::InventoryFile,
    mut visit: impl FnMut(usize, usize, &[usize], Vec<u8>) -> Result<()>,
) -> Result<(Vec<usize>, u64)> {
    let keys = airk_core::continuity::FactKeyIndex::new(&file.continuity);
    let order = order(artifact);
    let mut ranks = vec![0; order.len()];
    let mut page = 0;
    let mut start = 0;
    let mut group = Vec::new();
    let mut used = 0usize;
    let mut fact_bytes = 0u64;
    let mut framing = encode_page(artifact, &keys, page, &[])?.len();
    for (rank, ordinal) in order.into_iter().enumerate() {
        ranks[ordinal] = rank;
        let row = bounded_encoding(&RowRef {
            ordinal,
            fact: &artifact.facts[ordinal],
            continuity_key: keys.key(ordinal, &artifact.facts[ordinal]),
        })?;
        fact_bytes = fact_bytes
            .checked_add(row.len() as u64)
            .context("fact byte count overflow")?;
        let additional = row.len() + usize::from(!group.is_empty());
        if !group.is_empty()
            && (group.len() == ROWS
                || framing.saturating_add(used).saturating_add(additional) > TARGET_BYTES)
        {
            visit(
                page,
                start,
                &group,
                encode_page(artifact, &keys, page, &group)?,
            )?;
            start += group.len();
            page += 1;
            group.clear();
            used = 0;
            framing = encode_page(artifact, &keys, page, &[])?.len();
        }
        used = used
            .checked_add(row.len() + usize::from(!group.is_empty()))
            .context("native page byte overflow")?;
        group.push(ordinal);
    }
    if !group.is_empty() {
        visit(
            page,
            start,
            &group,
            encode_page(artifact, &keys, page, &group)?,
        )?;
    }
    Ok((ranks, fact_bytes))
}
impl FileStore {
    pub fn prepare_native_fact_projection(
        &self,
        artifact: &Artifact,
        file: &airk_core::publication::InventoryFile,
        mode: airk_core::compression::CompressionMode,
    ) -> Result<PreparedNativeProjection> {
        let metadata = airk_core::identity::framed_digest(
            b"current-file-metadata/1",
            &[&serde_json::to_vec(file)?],
        );
        let mut header = base(artifact, metadata)?;
        ensure!(
            artifact.facts.len() == header.facts,
            "native projection retained count mismatch"
        );
        let mut resources = Vec::new();
        let mut memory = 0usize;
        let (ranks, fact_bytes) = visit_pages(artifact, file, |_, start, ordinals, bytes| {
            let value = super::resources::prepare_resource(bytes, mode)?;
            memory = memory
                .checked_add(value.memory_bytes())
                .context("projection preparation memory overflow")?;
            if memory > MAX_PREPARED_NATIVE_BYTES {
                return Err(ProjectionPreparationLimit.into());
            }
            header.leaf_encoded_bytes = header
                .leaf_encoded_bytes
                .checked_add(value.reference.bytes as u64)
                .context("projection logical bytes overflow")?;
            header.leaf_stored_bytes = header
                .leaf_stored_bytes
                .checked_add(value.reference.stored_bytes())
                .context("projection stored bytes overflow")?;
            header.pages.push(NativeFactPageRef {
                start,
                rows: ordinals.len(),
                resource: value.reference.clone(),
            });
            resources.push(value);
            Ok(())
        })?;
        header.rank_by_ordinal = ranks;
        header.row_encoded_bytes = fact_bytes;
        let bytes = bounded_encoding(&header)?;
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err(ProjectionPreparationLimit.into());
        }
        let value = super::resources::prepare_resource(bytes, mode)?;
        memory = memory
            .checked_add(value.memory_bytes())
            .context("projection preparation memory overflow")?;
        if memory > MAX_PREPARED_NATIVE_BYTES {
            return Err(ProjectionPreparationLimit.into());
        }
        let root = value.reference.clone();
        resources.push(value);
        let edges = self.prepare_native_edge_projection(artifact, metadata, mode)?;
        let edges = if edges
            .root
            .projection
            .as_ref()
            .context("native edge summary missing")?
            .edges
            == 0
        {
            None
        } else {
            memory = memory
                .checked_add(edges.memory_bytes())
                .context("projection memory overflow")?;
            if memory > MAX_PREPARED_NATIVE_BYTES {
                return Err(ProjectionPreparationLimit.into());
            }
            resources.extend(edges.resources);
            Some(edges.root)
        };
        let lines = if let Some(lines) = self.prepare_native_line_projection(artifact, mode)? {
            memory = memory
                .checked_add(lines.memory_bytes())
                .context("line projection memory overflow")?;
            if memory > MAX_PREPARED_NATIVE_BYTES {
                return Err(ProjectionPreparationLimit.into());
            }
            let reference = lines.reference.clone();
            resources.push(lines);
            Some(reference)
        } else {
            None
        };
        Ok(PreparedNativeProjection {
            resources,
            root,
            edges,
            lines,
            memory,
        })
    }
    pub fn stage_optional_native_projections(
        &self,
        writer: &crate::WriterLock,
        quota: &Quota,
        plans: &[PreparedNativeProjection],
    ) -> Result<NativeProjectionAdmission> {
        super::gate::check_writer(self, writer)?;
        let mut unique = std::collections::BTreeMap::new();
        let mut required = 0u64;
        for value in plans.iter().flat_map(|plan| &plan.resources) {
            let digest = value.reference.object_digest()?;
            if let std::collections::btree_map::Entry::Vacant(entry) = unique.entry(digest) {
                if !self.prepared_resource_exists(value)? {
                    required = required
                        .checked_add(value.reference.bytes as u64)
                        .context("optional reservation size overflow")?;
                }
                entry.insert(value);
            }
        }
        let reservation = if required == 0 {
            None
        } else {
            match quota.reserve(self.collection_id()?, required) {
                Ok(guard) => {
                    guard.validate_collection_directory(self.directory())?;
                    Some(guard)
                }
                Err(error) if crate::capacity::is_storage_pressure(&error) => {
                    return Ok(NativeProjectionAdmission {
                        roots: None,
                        edge_roots: Vec::new(),
                        line_roots: Vec::new(),
                        reservation: None,
                    });
                }
                Err(error) => return Err(error),
            }
        };
        if required > 0 {
            writer.invalidate_census_before_mutation()?;
        }
        for value in unique.values() {
            self.retain_prepared_resource(value, reservation.as_ref(), None)?;
        }
        Ok(NativeProjectionAdmission {
            roots: Some(plans.iter().map(|plan| plan.root.clone()).collect()),
            edge_roots: plans.iter().map(|plan| plan.edges.clone()).collect(),
            line_roots: plans.iter().map(|plan| plan.lines.clone()).collect(),
            reservation,
        })
    }
    pub fn retain_native_fact_projection(
        &self,
        artifact: &Artifact,
        file: &airk_core::publication::InventoryFile,
        reservation: &Reservation,
    ) -> Result<ResourceReference> {
        self.retain_native_fact_projection_counted(
            artifact,
            file,
            reservation,
            airk_core::compression::CompressionMode::Off,
            None,
        )
    }
    pub fn retain_native_fact_projection_counted(
        &self,
        artifact: &Artifact,
        file: &airk_core::publication::InventoryFile,
        reservation: &Reservation,
        mode: airk_core::compression::CompressionMode,
        counts: Option<&mut super::resources::ResourceWriteCounts>,
    ) -> Result<ResourceReference> {
        let metadata = airk_core::identity::framed_digest(
            b"current-file-metadata/1",
            &[&serde_json::to_vec(file)?],
        );
        self.retain_native_fact_projection_bound(
            artifact,
            file,
            metadata,
            reservation,
            mode,
            counts,
        )
    }
    pub(crate) fn retain_native_fact_projection_bound(
        &self,
        artifact: &Artifact,
        file: &airk_core::publication::InventoryFile,
        metadata: Sha256Digest,
        reservation: &Reservation,
        mode: airk_core::compression::CompressionMode,
        mut counts: Option<&mut super::resources::ResourceWriteCounts>,
    ) -> Result<ResourceReference> {
        let mut header = base(artifact, metadata)?;
        ensure!(
            artifact.facts.len() == header.facts,
            "native projection retained count mismatch"
        );
        let (ranks, fact_bytes) = visit_pages(artifact, file, |_, start, ordinals, bytes| {
            header.leaf_encoded_bytes = header
                .leaf_encoded_bytes
                .checked_add(bytes.len() as u64)
                .context("native leaf byte overflow")?;
            let resource = self.retain_current_resource_encoded_counted(
                &bytes,
                reservation,
                mode,
                counts.as_deref_mut(),
            )?;
            header.leaf_stored_bytes = header
                .leaf_stored_bytes
                .checked_add(resource.stored_bytes())
                .context("native stored byte overflow")?;
            header.pages.push(NativeFactPageRef {
                start,
                rows: ordinals.len(),
                resource,
            });
            Ok(())
        })?;
        header.rank_by_ordinal = ranks;
        header.row_encoded_bytes = fact_bytes;
        let bytes = serde_json::to_vec(&header)?;
        ensure!(
            bytes.len() <= MAX_MANIFEST_BYTES,
            "native projection manifest exceeds metadata bound"
        );
        self.retain_current_resource_encoded_counted(&bytes, reservation, mode, counts)
    }
}
fn manifest(
    store: &FileStore,
    reference: &ResourceReference,
    artifact: &Artifact,
    file_metadata: Sha256Digest,
) -> Result<NativeFactHeader> {
    ensure!(
        reference.bytes <= MAX_MANIFEST_BYTES,
        "native projection manifest exceeds metadata bound"
    );
    let bytes = store.read_current_resource(reference)?;
    let value: NativeFactHeader = serde_json::from_slice(&bytes)?;
    let expected = base(artifact, file_metadata)?;
    ensure!(
        value.version == 1
            && value.path == expected.path
            && value.source == expected.source
            && value.payload == expected.payload
            && value.file_metadata == file_metadata
            && value.profile == expected.profile
            && value.facts == expected.facts
            && value.pages.len() <= MAX_FACTS
            && value.rank_by_ordinal.len() == value.facts
            && serde_json::to_vec(&value)? == bytes,
        "invalid native projection manifest binding"
    );
    let mut count = 0usize;
    let mut leaf_bytes = 0u64;
    let mut stored_bytes = 0u64;
    for page in &value.pages {
        ensure!(
            page.start == count
                && (1..=ROWS).contains(&page.rows)
                && page.resource.bytes <= super::resources::MAX_RESOURCE_BYTES,
            "invalid native projection page bounds"
        );
        count = count
            .checked_add(page.rows)
            .context("native projection count overflow")?;
        leaf_bytes = leaf_bytes
            .checked_add(page.resource.bytes as u64)
            .context("native projection bytes overflow")?;
        stored_bytes = stored_bytes
            .checked_add(page.resource.stored_bytes())
            .context("native projection stored bytes overflow")?;
    }
    ensure!(
        count == value.facts
            && leaf_bytes == value.leaf_encoded_bytes
            && stored_bytes == value.leaf_stored_bytes,
        "native projection manifest count mismatch"
    );
    let mut seen = vec![false; value.facts];
    for rank in &value.rank_by_ordinal {
        ensure!(
            *rank < value.facts && !seen[*rank],
            "invalid native projection ordinal ranks"
        );
        seen[*rank] = true;
    }
    Ok(value)
}
pub struct NativeFactRead<'a> {
    read: &'a CurrentRead,
    store: &'a FileStore,
    artifact: Artifact,
    reference: ResourceReference,
    header: NativeFactHeader,
}
impl NativeFactRead<'_> {
    pub fn header(&self) -> &NativeFactHeader {
        &self.header
    }
    pub fn artifact(&self) -> &Artifact {
        &self.artifact
    }
    pub fn manifest_bytes(&self) -> usize {
        self.reference.bytes
    }
    pub fn check_certificate(&self, certificate: &NativeProducerCertificate) -> Result<()> {
        self.read.token()?;
        ensure!(
            certificate.version == 1
                && self.reference.digest == certificate.manifest
                && self.header.file_metadata == certificate.file_metadata
                && self.header.source == certificate.source
                && self.header.payload == certificate.payload
                && self.header.profile == certificate.profile
                && self.header.facts == certificate.facts
                && self.header.pages.len() == certificate.pages,
            "native projection certification changed"
        );
        Ok(())
    }
    /// Pure representation formatting; only page() authenticates a stored row.
    pub fn hit(&self, row: &NativeFactRow) -> airk_core::FactHit {
        let mut hit = super::discovery::fact_hit(&self.artifact, &row.fact);
        hit.continuity_key = row.continuity_key;
        hit
    }
    pub fn stats(&self) -> NativeProjectionStats {
        NativeProjectionStats {
            facts: self.header.facts,
            leaf_objects: self.header.pages.len(),
            manifest_objects: 1,
            row_encoded_bytes: self.header.row_encoded_bytes,
            leaf_encoded_bytes: self.header.leaf_encoded_bytes,
            manifest_encoded_bytes: self.reference.bytes,
            leaf_stored_bytes: self.header.leaf_stored_bytes,
            manifest_stored_bytes: self.reference.stored_bytes(),
            retained_extraction_bytes: self
                .artifact
                .retained
                .as_ref()
                .map_or(0, |retained| retained.object_bytes),
            retained_extraction_stored_bytes: self.artifact.retained.as_ref().map_or(
                0,
                |retained| {
                    retained
                        .payload_encoding
                        .as_ref()
                        .map_or(retained.object_bytes as u64, |encoding| {
                            encoding.stored_bytes
                        })
                },
            ),
        }
    }
    pub(crate) fn retain_paths(
        &self,
        paths: &mut std::collections::BTreeSet<std::path::PathBuf>,
    ) -> Result<()> {
        self.read.token()?;
        paths.insert(
            std::path::PathBuf::from("current-resources")
                .join(format!("{}.json", self.reference.object_digest()?)),
        );
        for page in &self.header.pages {
            paths.insert(
                std::path::PathBuf::from("current-resources")
                    .join(format!("{}.json", page.resource.object_digest()?)),
            );
        }
        Ok(())
    }
    pub fn page_for_ordinal(&self, ordinal: usize) -> Result<usize> {
        let rank = *self
            .header
            .rank_by_ordinal
            .get(ordinal)
            .context("native projection ordinal unavailable")?;
        self.header
            .pages
            .partition_point(|page| page.start <= rank)
            .checked_sub(1)
            .context("native projection rank page missing")
    }
    pub fn page(&self, number: usize) -> Result<NativeFactPage> {
        self.read.token()?;
        let reference = self
            .header
            .pages
            .get(number)
            .context("native projection page unavailable")?;
        let bytes = self.store.read_current_resource(&reference.resource)?;
        let value: NativeFactPage = serde_json::from_slice(&bytes)?;
        ensure!(
            value.version == 1
                && value.page == number
                && value.rows.len() == reference.rows
                && serde_json::to_vec(&value)? == bytes,
            "native projection page binding mismatch"
        );
        for (offset, row) in value.rows.iter().enumerate() {
            ensure!(
                self.header.rank_by_ordinal.get(row.ordinal) == Some(&(reference.start + offset))
                    && row.fact.start_line > 0
                    && row.fact.end_line >= row.fact.start_line,
                "native projection row ordinal mismatch"
            );
        }
        Ok(value)
    }
}
/// Existing off-query upper bound, not the normal 64 MiB admitted turn.
/// Larger theoretical producer sums do not authorize automatic budget increases.
pub const MAX_PHYSICAL_AUDIT_TURN_BYTES: u64 = 512 * 1024 * 1024;
/// Historical logical certification, not physical audit coverage or read authority.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeProducerCertificate {
    pub version: u32,
    pub manifest: Sha256Digest,
    pub file_metadata: Sha256Digest,
    pub source: Sha256Digest,
    pub payload: Sha256Digest,
    pub profile: Sha256Digest,
    pub facts: usize,
    pub pages: usize,
}
pub struct FileProjectionCertification {
    pub scalars: bool,
    pub native: Option<NativeProducerCertificate>,
    pub native_edges: Option<super::query_projection::QueryProducerCertificate>,
    pub native_edges_empty: bool,
}
#[derive(Clone, Debug)]
pub struct NativeLeafAuditTurn {
    pub next_page: usize,
    pub checked_pages: usize,
    pub checked_bytes: u64,
    pub complete: bool,
    pub required_bytes: Option<u64>,
}
fn producer_matches(
    store: &FileStore,
    reference: &ResourceReference,
    artifact: &Artifact,
    file: &airk_core::publication::InventoryFile,
    file_metadata: Sha256Digest,
    physical: bool,
) -> Result<NativeFactHeader> {
    let header = manifest(store, reference, artifact, file_metadata)?;
    producer_matches_header(store, &header, artifact, file, physical)?;
    Ok(header)
}
fn producer_matches_header(
    store: &FileStore,
    header: &NativeFactHeader,
    artifact: &Artifact,
    file: &airk_core::publication::InventoryFile,
    physical: bool,
) -> Result<()> {
    ensure!(
        artifact.facts.len() == header.facts,
        "native producer fact count mismatch"
    );
    let mut order = vec![0usize; header.facts];
    for (ordinal, rank) in header.rank_by_ordinal.iter().enumerate() {
        order[*rank] = ordinal;
    }
    ensure!(
        order.windows(2).all(|pair| {
            let a = &artifact.facts[pair[0]];
            let b = &artifact.facts[pair[1]];
            (a.start_line, &a.kind, &a.name, pair[0]) < (b.start_line, &b.kind, &b.name, pair[1])
        }),
        "native producer canonical order mismatch"
    );
    let keys = airk_core::continuity::FactKeyIndex::new(&file.continuity);
    let mut row_bytes = 0u64;
    for (number, page) in header.pages.iter().enumerate() {
        let ordinals = &order[page.start..page.start + page.rows];
        let expected = encode_page(artifact, &keys, number, ordinals)?;
        ensure!(
            (expected.len() <= TARGET_BYTES || page.rows == 1)
                && page.resource.bytes == expected.len()
                && page.resource.digest
                    == airk_core::identity::framed_digest(b"current-resource/1", &[&expected]),
            "native projection producer logical page mismatch"
        );
        let framing =
            encode_page(artifact, &keys, number, &[])?.len() + page.rows.saturating_sub(1);
        row_bytes = row_bytes
            .checked_add(
                expected
                    .len()
                    .checked_sub(framing)
                    .context("native row framing mismatch")? as u64,
            )
            .context("native producer row size overflow")?;
        if physical {
            ensure!(
                store.read_current_resource(&page.resource)? == expected,
                "native projection producer physical page mismatch"
            );
        }
    }
    ensure!(
        row_bytes == header.row_encoded_bytes,
        "native producer encoded row count mismatch"
    );
    Ok(())
}
/// Writer certification and maintenance use the same strict producer input.
/// No method here grants private-root or executable authority.
pub(crate) fn verify_projection(
    store: &FileStore,
    reference: &ResourceReference,
    artifact: &Artifact,
    file: &airk_core::publication::InventoryFile,
    file_metadata: Sha256Digest,
) -> Result<()> {
    producer_matches(store, reference, artifact, file, file_metadata, true)?;
    Ok(())
}
impl CurrentRead {
    /// Content-free aggregate work/storage counters. Caller retains this same
    /// CurrentRead/token across pages; no source/extraction or fact rows are read.
    pub fn native_projection_stats_page(
        &self,
        store: &FileStore,
        after: Option<&str>,
        limit: usize,
    ) -> Result<NativeProjectionStatsPage> {
        ensure!(
            (1..=128).contains(&limit),
            "invalid native projection metric page"
        );
        let page = self.lookup_paths(super::postings::Lookup::Files, "", after, limit)?;
        let mut counts = NativeProjectionBatchStats::default();
        for path in page.paths {
            let stats = self
                .native_fact_projection(store, &path)?
                .map(|projection| projection.stats());
            counts.add(stats.as_ref())?;
        }
        Ok(NativeProjectionStatsPage {
            token: self.token()?,
            counts,
            next_after: page.next_after,
        })
    }
    pub fn native_fact_manifest_bytes(&self, path: &str) -> Result<Option<usize>> {
        Ok(self
            .native_fact_reference(path)?
            .map(|(reference, _)| reference.bytes))
    }
    pub fn native_fact_projection<'a>(
        &'a self,
        store: &'a FileStore,
        path: &str,
    ) -> Result<Option<NativeFactRead<'a>>> {
        let Some((reference, file_metadata)) = self.native_fact_reference(path)? else {
            return Ok(None);
        };
        let artifact = self
            .artifact(path)?
            .context("native projection artifact missing")?;
        let header = manifest(store, &reference, &artifact, file_metadata)?;
        Ok(Some(NativeFactRead {
            read: self,
            store,
            artifact,
            reference,
            header,
        }))
    }
    /// Admission size, not observed I/O: unique producer source/payload/metadata
    /// plus immutable manifest. Compute before any large certification read.
    pub fn projection_certification_bytes(&self, path: &str) -> Result<u64> {
        let artifact = self
            .artifact(path)?
            .context("projection certification artifact missing")?;
        let retained = artifact
            .retained
            .as_ref()
            .context("projection certification extraction missing")?;
        let native = self.native_fact_reference(path)?;
        if native.is_none() && self.file_scalars(path)?.is_none() {
            return (artifact.bytes as u64)
                .checked_add(retained.object_bytes as u64)
                .context("baseline certification size overflow");
        }
        let edges = self
            .native_edge_reference(path)?
            .map(|(reference, _)| -> Result<u64> {
                let summary = reference
                    .projection
                    .as_ref()
                    .context("native edge admission summary missing")?;
                summary.validate()?;
                (reference.bytes as u64)
                    .checked_add(summary.metadata_bytes as u64)
                    .context("native edge certification size overflow")
            })
            .transpose()?
            .unwrap_or(0);
        let manifest = native.map_or(0, |(reference, _)| reference.bytes as u64);
        let line_index = self
            .native_line_reference(path)?
            .map_or(0, |reference| reference.bytes as u64);
        let metadata = self.file_metadata_bytes(path)? as u64;
        (artifact.bytes as u64)
            .checked_add(retained.object_bytes as u64)
            .and_then(|bytes| bytes.checked_add(metadata))
            .and_then(|bytes| bytes.checked_add(manifest))
            .and_then(|bytes| bytes.checked_add(edges))
            .and_then(|bytes| bytes.checked_add(line_index))
            .context("projection certification size overflow")
    }
    /// One admitted source/payload/metadata pass for the historical object and
    /// both logical projection components. No physical native leaves are read.
    pub fn certify_file_projections(
        &self,
        store: &FileStore,
        path: &str,
    ) -> Result<FileProjectionCertification> {
        let native = self.native_fact_projection(store, path)?;
        self.certify_file_projections_with_native(store, path, native.as_ref())
    }
    pub fn certify_file_projections_with_native(
        &self,
        store: &FileStore,
        path: &str,
        native: Option<&NativeFactRead<'_>>,
    ) -> Result<FileProjectionCertification> {
        match (self.native_fact_reference(path)?, native) {
            (None, None) => {}
            (Some((reference, metadata)), Some(native)) => ensure!(
                std::ptr::eq(native.read, self)
                    && std::ptr::eq(native.store, store)
                    && native.header.path == path
                    && native.reference.digest == reference.digest
                    && native.header.file_metadata == metadata,
                "foreign prepared projection manifest"
            ),
            _ => anyhow::bail!("native certification manifest omitted"),
        }
        let compact = self
            .artifact(path)?
            .context("projection certification artifact missing")?;
        let retained = compact
            .retained
            .as_ref()
            .context("projection certification extraction missing")?;
        let selected = self.file_scalars(path)?;
        let utf8 = selected.is_some()
            && retained
                .profile
                .as_ref()
                .context("projection certification profile missing")?
                .contract_version
                == 2
            && compact.extraction_method == "utf8";
        let raw_lines = if let Some(reference) = self.native_line_reference(path)? {
            Some(store.verify_native_line_projection(&reference, &compact)?)
        } else if utf8 {
            Some(store.verify_retained_source_utf8_lines(&compact)?)
        } else {
            store.verify_retained_source_deep(&compact)?;
            None
        };
        let artifact = store.load_projection_artifact(&compact)?;
        if selected.is_none() && native.is_none() {
            return Ok(FileProjectionCertification {
                scalars: false,
                native: None,
                native_edges: None,
                native_edges_empty: false,
            });
        }
        let lines = if selected.is_none() {
            None
        } else {
            raw_lines.or_else(|| {
                Some(
                    artifact
                        .chunks
                        .iter()
                        .map(|chunk| chunk.end_line)
                        .max()
                        .unwrap_or(0),
                )
            })
        };
        let file = self.file_metadata(store, path)?;
        let scalars = self.audit_scalars_from_verified(path, &artifact, lines)?;
        let native = if let Some(native) = native {
            let file = file
                .as_ref()
                .context("native certification metadata missing")?;
            let header = native.header();
            producer_matches_header(store, header, &artifact, file, false)?;
            Some(NativeProducerCertificate {
                version: 1,
                manifest: native.reference.digest,
                file_metadata: header.file_metadata,
                source: header.source,
                payload: header.payload,
                profile: header.profile,
                facts: header.facts,
                pages: header.pages.len(),
            })
        } else {
            None
        };
        let native_edges = self
            .native_edge_reference(path)?
            .map(|(reference, metadata)| {
                store.certify_native_edges(&reference, &artifact, metadata)
            })
            .transpose()?;
        let native_edges_empty = self.native_edges_declared_empty(path)?;
        if native_edges_empty {
            store.verify_empty_native_edges(&artifact)?;
        }
        Ok(FileProjectionCertification {
            scalars,
            native,
            native_edges,
            native_edges_empty,
        })
    }
    pub fn certify_native_fact_projection(
        &self,
        store: &FileStore,
        path: &str,
    ) -> Result<Option<NativeProducerCertificate>> {
        let Some((reference, file_metadata)) = self.native_fact_reference(path)? else {
            return Ok(None);
        };
        let compact = self
            .artifact(path)?
            .context("native certification artifact missing")?;
        let artifact = store.load_projection_artifact(&compact)?;
        store.verify_retained_source(&compact)?;
        let file = self
            .file_metadata(store, path)?
            .context("native certification metadata missing")?;
        let header = producer_matches(store, &reference, &artifact, &file, file_metadata, false)?;
        Ok(Some(NativeProducerCertificate {
            version: 1,
            manifest: reference.digest,
            file_metadata,
            source: header.source,
            payload: header.payload,
            profile: header.profile,
            facts: header.facts,
            pages: header.pages.len(),
        }))
    }
    pub fn audit_native_fact_pages(
        &self,
        store: &FileStore,
        path: &str,
        certificate: &NativeProducerCertificate,
        after: usize,
        max_pages: usize,
        max_bytes: u64,
    ) -> Result<NativeLeafAuditTurn> {
        ensure!(
            (1..=128).contains(&max_pages)
                && (1..=MAX_PHYSICAL_AUDIT_TURN_BYTES).contains(&max_bytes),
            "invalid native leaf audit budget"
        );
        let (reference, metadata) = self
            .native_fact_reference(path)?
            .context("certified native projection disappeared")?;
        ensure!(
            certificate.version == 1
                && reference.digest == certificate.manifest
                && metadata == certificate.file_metadata,
            "native leaf audit certification changed"
        );
        if reference.bytes as u64 > max_bytes {
            return Ok(NativeLeafAuditTurn {
                next_page: after,
                checked_pages: 0,
                checked_bytes: 0,
                complete: false,
                required_bytes: Some(reference.bytes as u64),
            });
        }
        let projection = self
            .native_fact_projection(store, path)?
            .context("certified native projection unavailable")?;
        let header = projection.header();
        ensure!(
            header.source == certificate.source
                && header.payload == certificate.payload
                && header.profile == certificate.profile
                && header.facts == certificate.facts
                && header.pages.len() == certificate.pages
                && after <= header.pages.len(),
            "native leaf audit producer identity changed"
        );
        let mut bytes = reference.bytes as u64;
        let mut next = after;
        while next < header.pages.len() && next - after < max_pages {
            let leaf = header.pages[next].resource.bytes as u64;
            if bytes.saturating_add(leaf) > max_bytes {
                let required = (reference.bytes as u64)
                    .checked_add(leaf)
                    .context("native leaf audit size overflow")?;
                return Ok(NativeLeafAuditTurn {
                    next_page: next,
                    checked_pages: next - after,
                    checked_bytes: bytes,
                    complete: false,
                    required_bytes: (next == after).then_some(required),
                });
            }
            projection.page(next)?;
            bytes += leaf;
            next += 1;
        }
        Ok(NativeLeafAuditTurn {
            next_page: next,
            checked_pages: next - after,
            checked_bytes: bytes,
            complete: next == header.pages.len(),
            required_bytes: None,
        })
    }
    pub fn audit_native_fact_projection(&self, store: &FileStore, path: &str) -> Result<bool> {
        let Some((reference, file_metadata)) = self.native_fact_reference(path)? else {
            return Ok(false);
        };
        let compact = self
            .artifact(path)?
            .context("native audit artifact missing")?;
        let artifact = store.load_projection_artifact(&compact)?;
        store.verify_retained_source(&compact)?;
        let file = self
            .file_metadata(store, path)?
            .context("native audit file metadata missing")?;
        verify_projection(store, &reference, &artifact, &file, file_metadata)?;
        Ok(true)
    }
}
