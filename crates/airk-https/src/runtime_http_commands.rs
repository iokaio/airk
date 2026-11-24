// SPDX-License-Identifier: Apache-2.0
//! Setup-only HTTP discovery through the verified native service: the
//! endpoint read. Moved from
//! `runtime_install::http_commands` (split step 2b). Enrollment, credential
//! reads, revocation, ownership lookup and the `/mcp` probe were removed with
//! the credential (local trust plan B); predecessor cleanup keeps its own
//! read-only ledger lookup in `airk-installer`. The issuer trust commands
//! were removed with the loopback issuer (local trust plan C).
use crate::control_client;
use airk_core::service::{ControlMethod, ControlResponse, ServiceHealth};
use airk_core::service_http::EndpointDescriptor;
use airk_installer::runtime_install::*;
use anyhow::{Context, Result, bail, ensure};
use std::path::{Path, PathBuf};

async fn control(install: &Path) -> Result<(PathBuf, ServiceHealth)> {
    ensure!(
        !state_contract::required(install)?,
        "clean_break_required: the HTTP endpoint requires the initialized baseline"
    );
    healthy(installed_control(install).await?).await
}
/// The service's own health at a known control root, accepting requests.
async fn healthy(control: PathBuf) -> Result<(PathBuf, ServiceHealth)> {
    let response = control_client::request(&control, ControlMethod::Health).await?;
    let ControlResponse::Health { observation } = response else {
        return refused(response);
    };
    ensure!(observation.policy.accepting, "native service is draining");
    Ok((control, *observation))
}
fn refused<T>(response: ControlResponse) -> Result<T> {
    // The canonical native refusal remains complete in the diagnostic.
    bail!(
        "native HTTP setup refused: {}",
        serde_json::to_string(&response)?
    )
}
fn check_endpoint(
    control: &Path,
    endpoint: &EndpointDescriptor,
    health: &ServiceHealth,
) -> Result<()> {
    endpoint.validate()?;
    let binding = airk_endpoint::http_endpoint::installation_binding(control, endpoint.namespace)?;
    ensure!(
        endpoint.installation == binding.installation && endpoint.user == binding.user,
        "native HTTP installation binding changed"
    );
    ensure!(
        endpoint.service_instance == health.instance
            && endpoint.user == health.user
            && endpoint.executable_sha256.to_string() == health.executable_sha256,
        "native HTTP endpoint identity changed"
    );
    if let Some(clock) = health.catalog_clock {
        ensure!(
            endpoint.namespace == clock.namespace,
            "native HTTP endpoint namespace changed"
        );
    }
    Ok(())
}
pub async fn endpoint(install: &Path) -> Result<serde_json::Value> {
    let (control, health) = control(install).await?;
    checked_endpoint(control, health).await
}
/// M15a (split step 3e.2): the same read for the service's own control root.
pub async fn endpoint_at(control: PathBuf) -> Result<serde_json::Value> {
    let (control, health) = healthy(control).await?;
    checked_endpoint(control, health).await
}
async fn checked_endpoint(control: PathBuf, health: ServiceHealth) -> Result<serde_json::Value> {
    let response = control_client::request_for_instance(
        &control,
        ControlMethod::HttpEndpoint,
        health.instance,
    )
    .await?;
    let ControlResponse::HttpEndpoint { endpoint } = response else {
        return refused(response);
    };
    let endpoint = endpoint.context("native HTTP endpoint is not available")?;
    check_endpoint(&control, &endpoint, &health)?;
    Ok(serde_json::to_value(endpoint)?)
}
