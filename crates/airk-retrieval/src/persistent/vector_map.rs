// SPDX-License-Identifier: Apache-2.0
//! Authenticated occurrence-to-evidence mappings, separate from graph node identity.
use super::*;
use airk_core::{DocumentLocation, location::ByteSpan};
use tantivy::schema::Value;

pub(super) const MAX_RECORD_BYTES: usize = 8 << 20;
pub(super) const MAX_DECODED_BYTES: usize = 32 << 20;
pub(super) const MAX_POSTINGS: usize = 8192;
pub(super) const MAX_NODE_OCCURRENCES: usize = 128;
pub(super) const MAX_MEMBERS: usize = 64;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Member {
    pub evidence_id: Sha256Digest,
    pub lexical_ordinal: u64,
    pub region: Option<u32>,
    pub span: airk_core::extraction::LocalSpan,
    pub blank: bool,
    pub span_in_chunk: ByteSpan,
    pub start_line: usize,
    pub end_line: usize,
    pub location: DocumentLocation,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Body {
    pub region: Option<u32>,
    pub span: airk_core::extraction::LocalSpan,
    pub start_line: usize,
    pub end_line: usize,
    pub location: DocumentLocation,
    pub projection: Option<airk_core::mapped_binding::MappedBodyProjection>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Occurrence {
    pub path: String,
    pub payload_digest: Sha256Digest,
    pub source_hash: Sha256Digest,
    pub embedding_key: Sha256Digest,
    pub occurrence_id: Sha256Digest,
    pub body: Body,
    pub members: Vec<Member>,
}

fn identity<T: Serialize>(domain: &[u8], value: &T) -> Result<Sha256Digest> {
    let bytes = serde_json::to_vec(value)?;
    let mut hash = Sha256::new();
    hash.update((domain.len() as u64).to_le_bytes());
    hash.update(domain);
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
    Ok(Sha256Digest::from_bytes(hash.finalize().into()))
}

impl Member {
    pub fn set_identity(&mut self, path: &str, source_hash: Sha256Digest) -> Result<()> {
        self.evidence_id = self.identity(path, source_hash)?;
        Ok(())
    }
    fn identity(&self, path: &str, source_hash: Sha256Digest) -> Result<Sha256Digest> {
        identity(
            b"airk-vector-evidence/1",
            &(path, source_hash, self.region, &self.span),
        )
    }
}

impl Occurrence {
    pub fn new(
        path: String,
        payload_digest: Sha256Digest,
        source_hash: Sha256Digest,
        embedding_key: Sha256Digest,
        body: Body,
        members: Vec<Member>,
    ) -> Result<Self> {
        let mut value = Self {
            path,
            payload_digest,
            source_hash,
            embedding_key,
            occurrence_id: Sha256Digest::from_bytes([0; 32]),
            body,
            members,
        };
        value.occurrence_id = value.identity()?;
        value.validate(u64::MAX)?;
        Ok(value)
    }
    fn identity(&self) -> Result<Sha256Digest> {
        identity(
            b"airk-vector-occurrence/1",
            &(
                &self.path,
                self.payload_digest,
                self.embedding_key,
                self.body.region,
                &self.body.span,
                self.source_hash,
                crate::vector_units::VECTOR_UNIT_SPECIFICATION,
            ),
        )
    }
    pub fn validate(&self, chunks: u64) -> Result<()> {
        ensure!(
            !self.path.is_empty()
                && self.path.len() <= 4096
                && !self.path.starts_with('/')
                && !self.path.contains(['\\', ':', '\0'])
                && self
                    .path
                    .split('/')
                    .all(|part| !matches!(part, "" | "." | "..")),
            "invalid vector occurrence path"
        );
        ensure!(
            !self.members.is_empty() && self.members.len() <= MAX_MEMBERS,
            "vector occurrence member bound"
        );
        ensure!(
            self.body.span.start < self.body.span.end
                && self.body.span.end - self.body.span.start
                    <= crate::vector_units::MAX_VECTOR_UNIT_INPUT_BYTES
                && self.body.start_line > 0
                && self.body.start_line <= self.body.end_line,
            "invalid vector occurrence body"
        );
        match (&self.body.location, &self.body.projection) {
            (DocumentLocation::Mapped { region }, Some(projection)) => {
                region.validate_shape()?;
                ensure!(self.body.region.is_some(), "mapped body has no region");
                projection.validate(self.body.span.end - self.body.span.start)?;
            }
            (DocumentLocation::Mapped { .. }, None) => {
                anyhow::bail!("mapped body projection missing")
            }
            (_, None) => ensure!(
                self.body.region.is_none(),
                "mapped body lost original location"
            ),
            (_, Some(_)) => anyhow::bail!("unexpected mapped body projection"),
        }
        let mut previous = None;
        let mut cursor = self.body.span.start;
        for member in &self.members {
            ensure!(
                member.region == self.body.region && member.span.start == cursor,
                "vector body has a gap, overlap, or mixed region"
            );
            cursor = member.span.end;
            ensure!(
                member.lexical_ordinal < chunks
                    && member.span.start < member.span.end
                    && member.span.end <= airk_core::extraction::MAX_TEXT_BYTES
                    && member.span.end - member.span.start
                        == member
                            .span_in_chunk
                            .end
                            .saturating_sub(member.span_in_chunk.start)
                    && member.span_in_chunk.start < member.span_in_chunk.end
                    && member.span_in_chunk.end <= airk_core::extraction::MAX_TEXT_BYTES
                    && member.start_line > 0
                    && member.end_line >= member.start_line,
                "invalid vector evidence span"
            );
            let order = (member.lexical_ordinal, member.span_in_chunk.start);
            ensure!(
                previous.is_none_or(|prior| prior <= order),
                "unordered vector evidence members"
            );
            previous = Some((member.lexical_ordinal, member.span_in_chunk.end));
            if let DocumentLocation::Mapped { region } = &member.location {
                region.validate_shape()?;
                ensure!(
                    member.region.is_some(),
                    "mapped evidence has no local region"
                );
                if let airk_core::location::RegionLocation::Embedded { local_span, .. } = region {
                    ensure!(
                        local_span.start == member.span.start && local_span.end == member.span.end,
                        "mapped evidence local span mismatch"
                    );
                }
            } else {
                ensure!(
                    member.region.is_none(),
                    "region evidence lost mapped location"
                );
            }
            ensure!(
                member.evidence_id == member.identity(&self.path, self.source_hash)?,
                "vector evidence identity mismatch"
            );
        }
        ensure!(
            cursor == self.body.span.end,
            "vector body membership is incomplete"
        );
        ensure!(
            self.occurrence_id == self.identity()?,
            "vector occurrence identity mismatch"
        );
        Ok(())
    }
}

pub(super) struct Fields {
    pub graph_id: Field,
    pub path: Field,
    pub ordinal: Field,
    pub record: Field,
}

pub(super) fn schema() -> (Schema, Fields) {
    let mut builder = Schema::builder();
    let fields = Fields {
        graph_id: builder.add_u64_field("graph_id", INDEXED | FAST | STORED),
        path: builder.add_text_field("path", STRING),
        ordinal: builder.add_u64_field("map_ordinal", FAST),
        record: builder.add_bytes_field("record", STORED),
    };
    (builder.build(), fields)
}

pub(super) fn document(
    fields: &Fields,
    graph_id: u32,
    ordinal: u64,
    occurrence: &Occurrence,
) -> Result<TantivyDocument> {
    ensure!(graph_id > 0, "invalid mapped graph ID");
    let bytes = serde_json::to_vec(occurrence)?;
    ensure!(
        bytes.len() <= MAX_RECORD_BYTES,
        "vector occurrence record exceeds bound"
    );
    let mut document = TantivyDocument::default();
    document.add_u64(fields.graph_id, u64::from(graph_id));
    document.add_text(fields.path, &occurrence.path);
    document.add_u64(fields.ordinal, ordinal);
    document.add_bytes(fields.record, &bytes);
    Ok(document)
}

pub(super) fn open(directory: &Path, occurrences: usize) -> Result<(IndexReader, Fields)> {
    let (schema, fields) = schema();
    let index = Index::open_in_dir(directory.join("vector-map"))?;
    ensure!(
        index.schema() == schema,
        "unsupported vector map schema; rebuild with airk index --full"
    );
    let reader: IndexReader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::Manual)
        .try_into()?;
    ensure!(
        reader.searcher().num_docs() == occurrences as u64,
        "vector occurrence inventory mismatch"
    );
    Ok((reader, fields))
}

pub(super) fn read(
    reader: &IndexReader,
    fields: &Fields,
    address: tantivy::DocAddress,
    vectors: usize,
    chunks: u64,
    remaining: &mut usize,
) -> Result<Option<(u32, Occurrence)>> {
    let document: TantivyDocument = reader.searcher().doc(address)?;
    let id = document
        .get_first(fields.graph_id)
        .and_then(|value| value.as_u64())
        .context("missing mapped graph ID")?;
    ensure!(
        id > 0 && id <= vectors as u64,
        "mapped graph ID outside inventory"
    );
    let bytes = document
        .get_first(fields.record)
        .and_then(|value| value.as_bytes())
        .context("missing vector occurrence")?;
    ensure!(
        bytes.len() <= MAX_RECORD_BYTES,
        "vector occurrence record exceeds bound"
    );
    if bytes.len() > *remaining {
        return Ok(None);
    }
    *remaining -= bytes.len();
    let occurrence: Occurrence = serde_json::from_slice(bytes)?;
    occurrence.validate(chunks)?;
    Ok(Some((u32::try_from(id)?, occurrence)))
}
