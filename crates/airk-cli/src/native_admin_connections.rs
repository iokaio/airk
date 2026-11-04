// SPDX-License-Identifier: Apache-2.0
//! Fixed native administrative channels; evidence sessions never enter this pool.
use crate::{
    catalog_admission::NativeCatalogAdmission,
    project_supervisor::{AdministrativeKey, NativeAdministrativeConnection},
    service_management::Management,
};
use airk_core::{
    identity::Incarnation, native_reads::NativeRead, service::ProjectBinding, usage::ClientFamily,
};
use anyhow::{Context, Result, ensure};

pub(crate) struct NativeAdministrativePool {
    capacity: std::sync::Arc<airk_mcp::query_capacity::QueryCapacity>,
    slots: Vec<tokio::sync::Mutex<Option<NativeAdministrativeConnection>>>,
}
impl Default for NativeAdministrativePool {
    fn default() -> Self {
        Self {
            capacity: airk_mcp::query_capacity::QueryCapacity::new(16),
            slots: (0..16).map(|_| tokio::sync::Mutex::new(None)).collect(),
        }
    }
}
impl NativeAdministrativePool {
    pub(crate) async fn read(
        &self,
        management: &Management,
        gate: &NativeCatalogAdmission,
        caller: Incarnation,
        project: ProjectBinding,
        request: NativeRead,
        client: ClientFamily,
    ) -> Result<serde_json::Value> {
        let _permit = self
            .capacity
            .clone()
            .acquire_owned()
            .await
            .context("native administrative capacity unavailable")?;
        let clock = gate
            .catalog_clock()
            .context("native administrative clock unavailable")?;
        let worker = gate.administrative_worker(project)?;
        let expected = worker.map(|worker| AdministrativeKey {
            namespace: clock.namespace,
            worker,
            caller,
            client,
        });
        let mut selected = None;
        if let Some(key) = expected {
            for slot in &self.slots {
                if let Ok(state) = slot.try_lock()
                    && state
                        .as_ref()
                        .is_some_and(|connection| connection.reusable(key))
                {
                    selected = Some(state);
                    break;
                }
            }
        }
        if selected.is_none() {
            for slot in &self.slots {
                if let Ok(state) = slot.try_lock() {
                    selected = Some(state);
                    break;
                }
            }
        }
        let mut slot = selected.context("native administrative channel unavailable")?;
        // The future owns the live stream, so cancellation drops it instead of
        // caching a channel that may contain an accepted unread response.
        let cached = slot.take();
        let mut connection = match cached {
            Some(connection) if expected.is_some_and(|key| connection.reusable(key)) => connection,
            _ => {
                management
                    .request_administrative_read(project)
                    .await?
                    .connect(client)
                    .await?
            }
        };
        let key = connection.key();
        ensure!(
            key.namespace == clock.namespace
                && key.worker.project == project
                && key.caller == caller
                && key.client == client,
            "native administrative cache identity changed"
        );
        let result = connection.read(request).await;
        if result.is_ok() && connection.reusable(key) {
            *slot = Some(connection);
        }
        result
    }
}
