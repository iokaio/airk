// SPDX-License-Identifier: Apache-2.0
//! Client side of the native service control transport: versioned and
//! instance-checked control requests and the installer's initialized probe.
use crate::ipc::control::{registry_busy_retry, request_once};
use airk_core::service::{ControlMethod, ControlResponse};
use anyhow::{Result, ensure};
use std::path::Path;

pub async fn request(root: &Path, method: ControlMethod) -> Result<ControlResponse> {
    request_for_version(root, method, env!("CARGO_PKG_VERSION")).await
}

pub async fn request_for_version(
    root: &Path,
    method: ControlMethod,
    version: &str,
) -> Result<ControlResponse> {
    request_scoped(root, method, version, None).await
}
pub async fn request_for_instance(
    root: &Path,
    method: ControlMethod,
    expected: airk_core::identity::Incarnation,
) -> Result<ControlResponse> {
    request_scoped(root, method, env!("CARGO_PKG_VERSION"), Some(expected)).await
}
pub async fn request_for_version_and_instance(
    root: &Path,
    method: ControlMethod,
    version: &str,
    expected: airk_core::identity::Incarnation,
) -> Result<ControlResponse> {
    request_scoped(root, method, version, Some(expected)).await
}
/// Native optional work is never replayed after an uncertain dispatch.
pub async fn request_for_version_and_instance_once(
    root: &Path,
    method: ControlMethod,
    version: &str,
    expected: airk_core::identity::Incarnation,
) -> Result<ControlResponse> {
    crate::runtime_install::require_baseline_control(root)?;
    request_once(root, method, version, Some(expected)).await
}
pub fn prebaseline_metadata_method(method: &ControlMethod) -> Result<bool> {
    let encoded = serde_json::to_value(method)?;
    let name = encoded
        .as_str()
        .or_else(|| {
            encoded
                .as_object()
                .and_then(|object| object.keys().next().map(String::as_str))
        })
        .unwrap_or("");
    Ok(matches!(
        name,
        "health" | "shutdown" | "resources" | "projects" | "inactive_projects" | "service_events"
    ))
}

async fn request_scoped(
    root: &Path,
    method: ControlMethod,
    version: &str,
    expected_instance: Option<airk_core::identity::Incarnation>,
) -> Result<ControlResponse> {
    if !prebaseline_metadata_method(&method)? {
        crate::runtime_install::require_baseline_control(root)?;
    }
    // registry_busy means either an unaccepted metadata queue entry or a
    // classified read-only contention failure. Retain every request identity;
    // uncertain outcomes and other refusals never enter this retry contract.
    registry_busy_retry(|| request_once(root, method.clone(), version, expected_instance)).await
}

/// This transport accepts only the opaque installer proof and a fixed read.
/// Ordinary RequestEpoch, source reads and enrollment retain their baseline gate.
pub(crate) async fn request_initialized_probe(
    permit: &crate::runtime_install::InitializedProbe,
) -> Result<ControlResponse> {
    let (control, version, instance, namespace) = permit.request_identity()?;
    let response = registry_busy_retry(|| {
        request_once(
            control,
            ControlMethod::RequestEpoch,
            version,
            Some(instance),
        )
    })
    .await?;
    ensure!(
        matches!(&response, ControlResponse::RequestEpoch { namespace: observed, .. } if *observed == namespace),
        "clean-break bootstrap registry differs from initialized namespace"
    );
    Ok(response)
}
