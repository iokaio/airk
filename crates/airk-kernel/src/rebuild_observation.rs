// SPDX-License-Identifier: Apache-2.0
//! Read-only native context shared by status, diagnostics and configuration reads.
use crate::Kernel;
use airk_core::{
    rebuild::{RebuildObservation, RebuildUnavailable, RebuildUnavailableReason},
    service::ProjectBinding,
};
use anyhow::{Context, Result, ensure};
use std::sync::Arc;

pub(crate) struct RebuildObserver {
    binding: ProjectBinding,
    observe: Box<dyn Fn() -> Result<Option<RebuildObservation>> + Send + Sync>,
}

#[derive(Default)]
pub(crate) struct RebuildMetadata {
    pub observation: Option<RebuildObservation>,
    pub unavailable: Option<RebuildUnavailable>,
}

impl Kernel {
    /// The native project owner supplies the observer. Wire values cannot install
    /// it, grant a new source scope or start/replay a rebuild.
    pub fn with_rebuild_observer(
        mut self,
        binding: ProjectBinding,
        observe: impl Fn() -> Result<Option<RebuildObservation>> + Send + Sync + 'static,
    ) -> Self {
        self.rebuild_observer = Some(Arc::new(RebuildObserver {
            binding,
            observe: Box::new(observe),
        }));
        self
    }

    pub(crate) fn rebuild_metadata(&self) -> Result<RebuildMetadata> {
        let Some(observer) = &self.rebuild_observer else {
            return Ok(RebuildMetadata::default());
        };
        let Some(value) = (observer.observe)()? else {
            return Ok(RebuildMetadata::default());
        };
        let selected = value
            .scoped_for(observer.binding)?
            .context("native rebuild observation no longer includes this project")?;
        let index = &selected.indexes[0];
        let registration = if index.original.binding == observer.binding {
            &index.original
        } else {
            index
                .replacement
                .as_ref()
                .context("native rebuild replacement missing")?
        };
        ensure!(
            registration.root == self.root_id()
                && registration.collection == self.collection
                && std::path::Path::new(&registration.data_root) == self.data_root,
            "native rebuild observation belongs to another collection"
        );
        if self.read_scope.is_some() {
            // A complete recovery source roster is collection metadata. Keep
            // the same frozen/full-source permission required by settings reads.
            let read = self.current_reader()?;
            if let Err(error) = self.require_configured_index_metadata(&read, &index.sources) {
                if error.is::<airk_core::scoped_reads::ScopeReadRefused>() {
                    return Ok(RebuildMetadata {
                        observation: None,
                        unavailable: Some(RebuildUnavailable {
                            operation_id: selected.operation_id,
                            index_scope: observer.binding,
                            reason: RebuildUnavailableReason::PartialProjectScope,
                        }),
                    });
                }
                return Err(error);
            }
        }
        Ok(RebuildMetadata {
            observation: Some(selected),
            unavailable: None,
        })
    }

    /// Preserve the complete native value before canonical result pagination.
    pub fn attach_rebuild_observation(
        &self,
        mut value: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let metadata = self.rebuild_metadata()?;
        if let Some(observation) = metadata.observation {
            value
                .as_object_mut()
                .context("native rebuild metadata requires an object result")?
                .insert("rebuild".into(), serde_json::to_value(observation)?);
        }
        if let Some(unavailable) = metadata.unavailable {
            value
                .as_object_mut()
                .context("native rebuild metadata requires an object result")?
                .insert(
                    "rebuild_unavailable".into(),
                    serde_json::to_value(unavailable)?,
                );
        }
        Ok(value)
    }
}
