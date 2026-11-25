// SPDX-License-Identifier: Apache-2.0
//! One changed-path publication into the plain index: one SQLite transaction.
//!
//! Every publication of the current index goes through `CurrentUpdater` and
//! becomes one `PlainWriter::publish`. Extraction payloads travel with the
//! batch (`PlainPayloads`); a caller without them (producer imports, policy
//! trims, semantic rebinds) reuses the payload already stored for the file.
//! Vectors are not computed here: a configured model marks selected files
//! pending and selects the vector pipeline; embedding is a later step.
use airk_core::{model_objects::ModelObjectSet, vector_selection::VectorSelectionPolicy};
use airk_retrieval::model::{EmbeddingCacheContext, EmbeddingModel};
use airk_store::{
    FileStore, WriterLock,
    current::{
        ArtifactChange, CurrentCommit, CurrentScope, MAX_PENDING_INPUTS,
        graph::GraphStatistics,
        plain_writer::{
            CapturedPayload, PlainChange, PlainPublication, PlainRead, PlainResource, PlainUpsert,
        },
    },
    index::VectorSetup,
    quota::{Quota, Reservation},
};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;

/// Extractions captured by the scan, by path, waiting for their batch.
pub(crate) type PlainPayloads = BTreeMap<String, CapturedPayload>;

pub struct SemanticUpdate<'a> {
    pub model: &'a EmbeddingModel,
    pub objects: ModelObjectSet,
    pub selection: &'a VectorSelectionPolicy,
    pub cache: EmbeddingCacheContext<'a>,
    /// Publish exact evidence and retire incompatible vectors before inference.
    pub defer: bool,
}

pub struct LexicalPublicationWork {
    pub changed: airk_store::current::lexical::LexicalWork,
}

pub struct CurrentUpdateResult {
    pub commit: CurrentCommit,
    pub lexical: LexicalPublicationWork,
    pub graph: GraphStatistics,
    pub occurrences: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurrentUpdateStage {
    Prepared,
    Applying,
    Committed,
}

/// The caller owns the ordinary collection writer lock across planning and
/// publication. No live source is opened.
pub struct CurrentUpdater<'a> {
    pub store: &'a FileStore,
    pub quota: &'a Quota,
    pub scope: &'a CurrentScope,
    pub writer: &'a WriterLock,
    pub reservation: &'a Reservation,
    /// Applied configuration travels in the same transaction.
    pub configuration: Option<&'a airk_store::current::collection::CollectionConfiguration>,
}

fn path(change: &ArtifactChange) -> &str {
    match change {
        ArtifactChange::Upsert(input) => &input.artifact.path,
        ArtifactChange::Delete(path) => path,
    }
}

/// Options of one plain publication beyond its changes and resources.
#[derive(Default)]
pub(crate) struct PlainOptions<'a> {
    pub payloads: PlainPayloads,
    pub moves: Option<&'a BTreeMap<String, String>>,
    /// Recount every file after this batch (a configuration change).
    pub recount: bool,
    /// The vector dimensions of the configured model, when known.
    pub dimensions: Option<usize>,
}

impl CurrentUpdater<'_> {
    /// Apply a bounded sorted batch. An empty/no-op scan is handled by the
    /// caller without publication; an explicit full rebuild supplies all paths.
    pub fn apply(
        &self,
        changes: &[ArtifactChange],
        semantic: Option<SemanticUpdate<'_>>,
    ) -> Result<CurrentUpdateResult> {
        self.apply_observed(changes, semantic, &mut |_| {})
    }

    /// Progress notifications occur only after their durable boundary. An
    /// observer cannot veto a successful commit; it must not reenter the store
    /// while the writer gate is held.
    pub fn apply_observed(
        &self,
        changes: &[ArtifactChange],
        semantic: Option<SemanticUpdate<'_>>,
        observer: &mut dyn FnMut(CurrentUpdateStage),
    ) -> Result<CurrentUpdateResult> {
        let options = PlainOptions {
            dimensions: semantic.as_ref().map(|s| s.model.dimensions()),
            ..Default::default()
        };
        self.apply_plain(changes, &[], options, observer)
    }

    pub fn apply_with_resources(
        &self,
        changes: &[ArtifactChange],
        semantic: Option<SemanticUpdate<'_>>,
        resources: &[airk_store::current::resources::ResourceChange],
    ) -> Result<CurrentUpdateResult> {
        let options = PlainOptions {
            dimensions: semantic.as_ref().map(|s| s.model.dimensions()),
            ..Default::default()
        };
        self.apply_plain(changes, resources, options, &mut |_| {})
    }

    /// One scan batch, with the extractions the scan captured for it.
    pub(crate) fn apply_scanned(
        &self,
        changes: &[ArtifactChange],
        resources: &[airk_store::current::resources::ResourceChange],
        options: PlainOptions<'_>,
    ) -> Result<CurrentUpdateResult> {
        self.apply_plain(changes, resources, options, &mut |_| {})
    }

    fn apply_plain(
        &self,
        changes: &[ArtifactChange],
        resources: &[airk_store::current::resources::ResourceChange],
        mut options: PlainOptions<'_>,
        observer: &mut dyn FnMut(CurrentUpdateStage),
    ) -> Result<CurrentUpdateResult> {
        let _publish = airk_core::stage_timing::scope("publish");
        crate::service_execution::progress_phase(airk_core::service_progress::WorkPhase::Preparing);
        ensure!(
            changes.len() <= MAX_PENDING_INPUTS
                && changes
                    .windows(2)
                    .all(|pair| path(&pair[0]) < path(&pair[1])),
            "invalid current changed-path batch"
        );
        if !self.store.plain_index_present()? {
            let configuration = self
                .configuration
                .context("the index is not initialized; run airk index")?;
            self.store
                .plain_create(self.writer, self.scope, configuration)?;
        }
        let read = self.store.plain_read()?;
        ensure!(
            read.scope() == self.scope,
            "current publication scope differs from the index"
        );
        let applied = read.collection_state()?;
        let configuration = self
            .configuration
            .or(applied.as_ref().map(|state| &state.configuration));
        let next = read
            .sequence()
            .checked_add(1)
            .context("current sequence overflow")?;
        let resources = self.plain_resources(&read, resources, changes, next)?;
        let roots = configuration.map(|c| c.roots.clone()).unwrap_or_default();
        let model = configuration.and_then(|c| c.model.as_ref());
        let selection = configuration.map(|c| &c.vector_selection);
        let mut plain = Vec::with_capacity(changes.len());
        let mut metadata_bytes = 0usize;
        for change in changes {
            match change {
                ArtifactChange::Delete(path) => plain.push(PlainChange::Delete(path.clone())),
                ArtifactChange::Upsert(input) => {
                    let artifact = &input.artifact;
                    let captured = match options.payloads.remove(&artifact.path) {
                        Some(captured) => captured,
                        None => self.stored_payload(&read, artifact)?,
                    };
                    let moved = options.moves.and_then(|moves| moves.get(&artifact.path));
                    let file = {
                        let _stage = airk_core::stage_timing::scope("publish.metadata");
                        super::metadata::prepare(
                            &read,
                            &roots,
                            input,
                            &captured,
                            moved.map(String::as_str),
                            next,
                        )
                        .context("prepare current metadata")?
                    };
                    metadata_bytes = metadata_bytes
                        .checked_add(serde_json::to_vec(&file)?.len())
                        .context("current metadata budget overflow")?;
                    ensure!(
                        metadata_bytes <= airk_store::current::MAX_CURRENT_METADATA_READ_BYTES,
                        "current captured metadata admission exceeded"
                    );
                    let vector_pending = match (model, selection) {
                        (Some(_), Some(selection)) => {
                            let profile = artifact
                                .retained
                                .as_ref()
                                .and_then(|retained| retained.profile.as_ref())
                                .context("current profile missing")?;
                            selection
                                .decision(&artifact.path, profile, &artifact.language)?
                                .eligible
                        }
                        _ => false,
                    };
                    plain.push(PlainChange::Upsert(Box::new(PlainUpsert {
                        input: input.as_ref().clone(),
                        file,
                        captured,
                        vector_pending,
                    })));
                }
            }
        }
        drop(read);
        let mut writer = self.store.plain_writer(self.writer)?;
        // Vectors follow the applied model: a new pipeline starts an empty
        // generation and returns every selected file to pending.
        if self.configuration.is_some() {
            match model {
                Some(objects) => {
                    if let Some(dimensions) = options.dimensions {
                        writer.configure_vectors(Some(&VectorSetup {
                            dimensions: u32::try_from(dimensions)?,
                            pipeline: objects.pipeline_digest,
                        }))?;
                    }
                }
                None => {
                    writer.configure_vectors(None)?;
                }
            }
        }
        crate::service_execution::progress_prepared(changes.len() as u64);
        observer(CurrentUpdateStage::Prepared);
        crate::service_execution::progress_phase(
            airk_core::service_progress::WorkPhase::Publishing,
        );
        observer(CurrentUpdateStage::Applying);
        let committed = writer
            .publish(PlainPublication {
                changes: plain,
                resources,
                configuration: self.configuration,
                recount: std::mem::take(&mut options.recount),
            })
            .context("publish current batch")?;
        crate::service_execution::progress_committed(changes.len() as u64);
        observer(CurrentUpdateStage::Committed);
        Ok(CurrentUpdateResult {
            commit: CurrentCommit {
                token: committed.token,
                maintenance_error: None,
            },
            lexical: LexicalPublicationWork {
                changed: airk_store::current::lexical::LexicalWork {
                    changed_artifacts: committed.changed_artifacts,
                    added_documents: committed.chunks_added,
                },
            },
            graph: GraphStatistics::default(),
            occurrences: 0,
        })
    }

    /// The batch form of an extraction that the caller did not capture: the
    /// payload this index stores under the record's digest.
    fn stored_payload(
        &self,
        read: &PlainRead,
        artifact: &airk_core::Artifact,
    ) -> Result<CapturedPayload> {
        let mut compact = artifact.clone();
        compact.chunks.clear();
        compact.facts.clear();
        read.captured(&compact)
            .context("extraction payload unavailable; rebuild the index")
    }
    /// Resource bodies for the batch. The source-control witness is bound to
    /// the publication it describes, as before.
    pub(super) fn plain_resources(
        &self,
        read: &PlainRead,
        resources: &[airk_store::current::resources::ResourceChange],
        changes: &[ArtifactChange],
        next: u64,
    ) -> Result<Vec<PlainResource>> {
        let control_key = airk_core::source_control::CONTROL_BASELINE_RESOURCE;
        let mut keys = std::collections::BTreeSet::new();
        let mut plain = Vec::with_capacity(resources.len() + 1);
        for resource in resources {
            ensure!(
                airk_store::current::resources::KEYS.contains(&resource.key.as_str())
                    && keys.insert(resource.key.as_str()),
                "unknown or duplicate current resource"
            );
            if resource.key == "embedding-cache" {
                continue;
            }
            let body = resource
                .reference
                .as_ref()
                .map(|reference| self.store.take_current_resource(reference))
                .transpose()?;
            let body = if resource.key == control_key {
                body.map(|bytes| self.bind_control(&bytes, next))
                    .transpose()?
            } else {
                body
            };
            plain.push(PlainResource {
                key: resource.key.clone(),
                body,
            });
        }
        if !keys.contains(control_key) {
            let pure_semantic = changes.is_empty()
                && self.configuration.is_none()
                && !resources.is_empty()
                && resources.iter().all(|resource| {
                    matches!(
                        resource.key.as_str(),
                        "semantic-work" | "semantic-policy" | "embedding-cache"
                    )
                });
            let mut carried = None;
            if pure_semantic && let Some(bytes) = read.resource(control_key)? {
                let baseline: airk_core::source_control::SourceControlBaseline =
                    serde_json::from_slice(&bytes)?;
                baseline.validate()?;
                let same_configuration = read.collection_state()?.is_some_and(|state| {
                    serde_json::to_vec(&state.configuration).is_ok_and(|configuration| {
                        baseline.configuration
                            == airk_core::identity::framed_digest(
                                b"airk/refresh-applied-configuration/1",
                                &[&configuration],
                            )
                    })
                });
                if same_configuration
                    && baseline.matches_publication(read.scope().incarnation, read.sequence())
                {
                    carried = Some(self.bind_control(&bytes, next)?);
                }
            }
            plain.push(PlainResource {
                key: control_key.into(),
                body: carried,
            });
        }
        Ok(plain)
    }

    fn bind_control(&self, bytes: &[u8], next: u64) -> Result<Vec<u8>> {
        ensure!(
            bytes.len() <= airk_core::source_control::MAX_CONTROL_BASELINE_BYTES,
            "retained control witness exceeds bound"
        );
        let mut baseline: airk_core::source_control::SourceControlBaseline =
            serde_json::from_slice(bytes)?;
        baseline.validate()?;
        ensure!(
            serde_json::to_vec(&baseline)? == bytes,
            "noncanonical retained control baseline"
        );
        baseline.bind_publication(self.scope.incarnation, next)?;
        Ok(serde_json::to_vec(&baseline)?)
    }
}
