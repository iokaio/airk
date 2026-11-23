// SPDX-License-Identifier: Apache-2.0
//! Setup-only HTTP discovery through the verified native service: the
//! endpoint read and the issuer trust inspection. Moved from
//! `runtime_install::http_commands` (split step 2b). Enrollment, credential
//! reads, revocation, ownership lookup and the `/mcp` probe were removed with
//! the credential (local trust plan B); predecessor cleanup keeps its own
//! read-only ledger lookup in `airk-installer`.
use crate::control_client;
use airk_core::identity::Sha256Digest;
use airk_core::service::{ControlMethod, ControlResponse, ServiceHealth};
use airk_core::service_http::EndpointDescriptor;
use airk_installer::runtime_install::*;
use anyhow::{Context, Result, bail, ensure};
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

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
    let endpoint = endpoint.context("native HTTPS endpoint is not available")?;
    check_endpoint(&control, &endpoint, &health)?;
    Ok(serde_json::to_value(endpoint)?)
}
/// Inspection authenticates existing native metadata and observes the live
/// service. It never elects a service, provisions a CA or enrolls a credential.
pub async fn trust(
    install: &Path,
    action: HttpTrustAction,
    expected_ca: Option<Sha256Digest>,
) -> Result<serde_json::Value> {
    ensure!(
        action == HttpTrustAction::Inspect || expected_ca.is_some(),
        "install/remove requires the reviewed issuer SHA256"
    );
    let control = installed_metadata_control(install)?;
    if action != HttpTrustAction::Install {
        let status = tokio::task::spawn_blocking(move || {
            let (binding, endpoint) =
                airk_endpoint::http_endpoint::trust::saved_identity(&control)?;
            check_reviewed_ca(action, expected_ca, endpoint.ca_sha256)?;
            airk_endpoint::http_endpoint::trust::manage(
                &control.join("http-1"),
                binding,
                endpoint.ca_sha256,
                if action == HttpTrustAction::Remove {
                    airk_endpoint::http_endpoint::trust::Action::Remove
                } else {
                    airk_endpoint::http_endpoint::trust::Action::Inspect
                },
            )
        })
        .await??;
        return Ok(serde_json::to_value(status)?);
    }
    let approval = match managed::read(install)? {
        Some(policy) => policy.metadata_approval()?,
        None => selected_preflight(install)?.approval,
    };
    let response = control_client::request(&control, ControlMethod::Health).await?;
    let ControlResponse::Health { observation } = response else {
        return refused(response);
    };
    require_matching_health(&approval, &observation)?;
    let response = control_client::request_for_instance(
        &control,
        ControlMethod::HttpEndpoint,
        observation.instance,
    )
    .await?;
    let ControlResponse::HttpEndpoint { endpoint } = response else {
        return refused(response);
    };
    let endpoint = endpoint.context(
        "native HTTPS endpoint is not available; complete native setup before managing user trust",
    )?;
    check_endpoint(&control, &endpoint, &observation)?;
    check_reviewed_ca(action, expected_ca, endpoint.ca_sha256)?;
    let binding = airk_endpoint::http_endpoint::installation_binding(&control, endpoint.namespace)?;
    let action = match action {
        HttpTrustAction::Inspect => airk_endpoint::http_endpoint::trust::Action::Inspect,
        HttpTrustAction::Install => airk_endpoint::http_endpoint::trust::Action::Install,
        HttpTrustAction::Remove => airk_endpoint::http_endpoint::trust::Action::Remove,
    };
    let root = control.join("http-1");
    let status = tokio::task::spawn_blocking(move || {
        airk_endpoint::http_endpoint::trust::manage(&root, binding, endpoint.ca_sha256, action)
    })
    .await??;
    Ok(serde_json::to_value(status)?)
}

fn check_reviewed_ca(
    action: HttpTrustAction,
    expected: Option<Sha256Digest>,
    current: Sha256Digest,
) -> Result<()> {
    ensure!(
        action == HttpTrustAction::Inspect || expected.is_some(),
        "install/remove requires the reviewed issuer SHA256"
    );
    ensure!(
        expected.is_none_or(|reviewed| reviewed == current),
        "native HTTPS issuer changed after review"
    );
    Ok(())
}
