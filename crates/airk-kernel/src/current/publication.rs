// SPDX-License-Identifier: Apache-2.0
//! Current publication for explicit producer and OCR transactions.
use super::{CurrentUpdater, SemanticUpdate};
use crate::Kernel;
use airk_core::Snapshot;
use airk_store::{
    WriterLock,
    current::{
        ArtifactChange, CapturedInput, analysis::AnalysisEvidence, resources::ResourceChange,
    },
    quota::{Quota, Reservation},
};
use anyhow::{Context, Result, ensure};

impl Kernel {
    pub(crate) fn publish_current_projection(
        &self,
        writer: &WriterLock,
        snapshot: &Snapshot,
        reservation: &Reservation,
    ) -> Result<u64> {
        let read = self.current_reader()?;
        ensure!(
            snapshot.root == self.root_id() && snapshot.collection == self.collection,
            "foreign current publication"
        );
        let previous = self.current_scan_projection(&read)?;
        let configuration = read
            .collection_state()?
            .context("current configuration missing")?
            .configuration;
        let mut changes = std::collections::BTreeMap::new();
        for (path, artifact) in &snapshot.artifacts {
            if previous
                .artifacts
                .get(path)
                .map(serde_json::to_vec)
                .transpose()?
                != Some(serde_json::to_vec(artifact)?)
            {
                changes.insert(
                    path.clone(),
                    ArtifactChange::Upsert(Box::new(CapturedInput::new(artifact.clone()))),
                );
            }
        }
        for path in previous
            .artifacts
            .keys()
            .filter(|path| !snapshot.artifacts.contains_key(*path))
        {
            changes.insert(path.clone(), ArtifactChange::Delete(path.clone()));
        }
        let reference = if let Some(manifest) = &snapshot.analysis {
            ensure!(
                manifest.source_hashes == airk_ingest::semantic::source_inventory(snapshot),
                "producer inventory differs from current artifacts"
            );
            let analysis = AnalysisEvidence {
                manifest: manifest.clone(),
                facts: snapshot.imported_facts.clone(),
                relationships: snapshot.imported_relationships.clone(),
            };
            Some(
                self.store
                    .retain_current_resource(&analysis.canonical_bytes()?, reservation)?,
            )
        } else {
            None
        };
        let scope = read.head().scope.clone();
        drop(read);
        let model = if changes
            .values()
            .any(|c| matches!(c, ArtifactChange::Upsert(_)))
        {
            configuration
                .model
                .as_ref()
                .map(|m| {
                    airk_retrieval::model::EmbeddingModel::open_objects(self.store.directory(), m)
                })
                .transpose()?
        } else {
            None
        };
        let mut stats = airk_retrieval::model::EmbeddingCacheStats::default();
        let semantic = model.as_ref().map(|model| SemanticUpdate {
            model,
            objects: configuration.model.clone().expect("loaded model"),
            selection: &configuration.vector_selection,
            defer: false,
            cache: airk_retrieval::model::EmbeddingCacheContext {
                cache: None,
                bypass: false,
                stats: &mut stats,
                max_workers: 1,
                working_memory_bytes: 256 << 20,
            },
        });
        let quota = Quota::open(&self.data_root)?;
        let result = CurrentUpdater {
            store: &self.store,
            quota: &quota,
            scope: &scope,
            writer,
            reservation,
            configuration: Some(&configuration),
        }
        .apply_with_resources(
            &changes.into_values().collect::<Vec<_>>(),
            semantic,
            &[ResourceChange {
                key: "analysis".into(),
                reference,
            }],
        )?;
        Ok(result.commit.token.sequence)
    }
}
