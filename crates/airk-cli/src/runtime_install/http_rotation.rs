// SPDX-License-Identifier: Apache-2.0
//! Reviewed issuer migration over native management; no HTTP request can invoke it.
use super::*;
use crate::http_endpoint::{
    self, TlsIdentity,
    rotation::{self, Phase, Record, now_ms, output, saved_endpoint},
};
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service_http::EndpointDescriptor,
};

async fn current_endpoint(install: &Path, control: &Path) -> Result<EndpointDescriptor> {
    let saved = snapshot::identity(install)?;
    let ControlResponse::Health { observation } =
        control_client::request(control, ControlMethod::Health).await?
    else {
        bail!("issuer rotation requires authenticated live service health");
    };
    require_matching_health(&saved.approval, &observation)?;
    ensure!(
        observation.executable_sha256 == saved.executable_sha256,
        "issuer service image changed"
    );
    let ControlResponse::HttpEndpoint { endpoint } = control_client::request_for_instance(
        control,
        ControlMethod::HttpEndpoint,
        observation.instance,
    )
    .await?
    else {
        bail!("issuer endpoint unavailable");
    };
    let endpoint = *endpoint.context("issuer endpoint unavailable")?;
    let binding = http_endpoint::installation_binding(control, endpoint.namespace)?;
    ensure!(
        endpoint.service_instance == observation.instance
            && endpoint.installation == binding.installation
            && endpoint.user == binding.user
            && endpoint.executable_sha256.to_string() == observation.executable_sha256,
        "issuer endpoint crossed native owner"
    );
    endpoint.validate()?;
    Ok(endpoint)
}

pub(crate) struct Input {
    pub action: HttpIssuerRotationAction,
    pub operation: Option<Incarnation>,
    pub expected_ca: Option<Sha256Digest>,
    pub expected_descriptor: Option<Sha256Digest>,
    pub expected_receipt: Option<Sha256Digest>,
    pub overlap_seconds: u32,
}
pub(crate) async fn run(install: &Path, input: Input) -> Result<serde_json::Value> {
    let control = installed_metadata_control(install)?;
    if input.action == HttpIssuerRotationAction::Status {
        return output(&control, None);
    }
    let operation = input
        .operation
        .context("issuer rotation requires a reviewed operation ID")?;
    let _install_lock = install_lock(install)?;
    ensure!(
        installed_metadata_control(install)? == control,
        "issuer installation control changed"
    );
    let _rotation_lock = rotation::lock(&control)?;
    if input.action == HttpIssuerRotationAction::Prepare {
        managed::require_unmanaged(install)?;
        ensure!(
            activation::pending(install)?.is_none(),
            "pending activation owns runtime maintenance"
        );
        lifecycle::require_settled(install)?;
        lifecycle::require_start_allowed(install)?;
        let selected = snapshot::identity(install)?;
        let (main, _) = verify_runtime(install, &selected.selection.active)?;
        ensure!(
            runtime_pair::single_executable(&main)?,
            "issuer rotation requires a modern approved runtime"
        );
        let (endpoint, basis, _exclusion) = match current_endpoint(install, &control).await {
            Ok(endpoint) => {
                ensure!(
                    endpoint.state == airk_core::service_http::EndpointState::Ready,
                    "issuer service is not accepting requests"
                );
                (endpoint, rotation::PreparationBasis::LiveNativeHealth, None)
            }
            Err(error) => {
                // Saved metadata is never promoted to live health. Actual native
                // exclusion permits recovery when an expired issuer prevents TLS startup.
                let exclusion = quiescent(install).context(error)?;
                let saved = snapshot::identity(install)?;
                let (main, _) = verify_runtime(install, &saved.selection.active)?;
                ensure!(
                    runtime_pair::single_executable(&main)?,
                    "issuer rotation requires a modern approved runtime"
                );
                let endpoint =
                    saved_endpoint(&control)?.context("saved issuer endpoint unavailable")?;
                ensure!(
                    endpoint.executable_sha256.to_string() == saved.executable_sha256,
                    "saved issuer endpoint does not belong to the selected approved image"
                );
                (
                    endpoint,
                    rotation::PreparationBasis::StoppedNativeExclusion,
                    Some(exclusion),
                )
            }
        };
        rotation::prepare(
            &control,
            operation,
            endpoint,
            rotation::PrepareReview {
                expected_ca: input
                    .expected_ca
                    .context("prepare requires the reviewed old CA digest")?,
                expected_descriptor: input
                    .expected_descriptor
                    .context("prepare requires the reviewed descriptor digest")?,
                overlap_seconds: input.overlap_seconds,
                now: now_ms()?,
                basis,
            },
        )?;
        return output(&control, None);
    }
    let mut record = rotation::reviewed(
        &control,
        operation,
        input
            .expected_receipt
            .context("issuer rotation requires its reviewed receipt digest")?,
    )?;
    match input.action {
        HttpIssuerRotationAction::Status | HttpIssuerRotationAction::Prepare => unreachable!(),
        HttpIssuerRotationAction::Abort => {
            if record.phase == Phase::Aborted {
                return output(&control, None);
            }
            ensure!(
                record.phase == Phase::Prepared,
                "only an unswitched prepared issuer can be aborted"
            );
            let active = TlsIdentity::inspect_bundle(&control.join("http-1"), &record.binding())?;
            ensure!(
                active.ca_sha256 == record.old_ca_sha256,
                "active issuer changed; preserve rotation for recovery"
            );
            let root = record.new_root()?.to_path_buf();
            let binding = record.binding();
            let sha = record.new_ca_sha256;
            let result = tokio::task::spawn_blocking(move || {
                http_endpoint::trust::abort_rotation_new(&root, binding, sha)
            })
            .await??;
            TlsIdentity::retire_staged_private_key(record.new_root()?, &record.new_bundle)?;
            TlsIdentity::retire_staged_private_key(record.old_root()?, &record.old_bundle)?;
            rotation::advance(&control, &record, Phase::Aborted, now_ms()?, None)?;
            output(&control, result.map(serde_json::to_value).transpose()?)
        }
        HttpIssuerRotationAction::TrustNew => {
            ensure!(
                record.phase == Phase::Prepared,
                "new issuer trust must be reviewed before commit"
            );
            // Only the exact staged public issuer reaches the existing OS adapter.
            let actual = TlsIdentity::inspect_bundle(record.new_root()?, &record.binding())?;
            ensure!(
                actual == record.new_bundle,
                "staged issuer changed after review"
            );
            let root = record.new_root()?.to_path_buf();
            let binding = record.binding();
            let sha = record.new_ca_sha256;
            let status = tokio::task::spawn_blocking(move || {
                http_endpoint::trust::manage(
                    &root,
                    binding,
                    sha,
                    http_endpoint::trust::Action::Install,
                )
            })
            .await??;
            output(&control, Some(serde_json::to_value(status)?))
        }
        HttpIssuerRotationAction::Commit => {
            ensure!(
                !matches!(record.phase, Phase::Finalized | Phase::Aborted),
                "issuer rotation is already terminal"
            );
            if record.phase == Phase::Activated {
                return output(&control, None);
            }
            if record.phase == Phase::Prepared {
                record = rotation::advance(&control, &record, Phase::Stopping, now_ms()?, None)?;
            }
            // A previous commit may have started the exact replacement before its
            // reply/journal acknowledgment. Never checkpoint that fresh owner as old.
            if record.phase == Phase::Restarting {
                if let Ok(ControlResponse::Health { observation }) =
                    control_client::request(&control, ControlMethod::Health).await
                    && observation.instance != record.old_endpoint.service_instance
                {
                    finish_started(install, &control, &record, &observation).await?;
                    return output(&control, None);
                }
                if let Some(health) =
                    issuer_lifecycle::recover_committed_successor(install, &record).await?
                {
                    finish_started(install, &control, &record, &health).await?;
                    return output(&control, None);
                }
            }
            let stopped = issuer_lifecycle::prepare_stop(
                install,
                operation,
                record.old_endpoint.service_instance,
                record.old_endpoint.executable_sha256,
            )
            .await?;
            ensure!(
                stopped.control() == control,
                "issuer stopped control changed"
            );
            if record.phase == Phase::Stopping {
                let current = TlsIdentity::snapshot_current_predecessor(
                    &control.join("http-1"),
                    record.old_root()?,
                    &record.activation_old_root()?,
                    &record.old_bundle,
                    stopped.service_lease(),
                )?;
                record = rotation::record_activation_predecessor(
                    &control,
                    &record,
                    current,
                    serde_json::to_value(stopped.proof())?,
                )?;
                record = rotation::advance(&control, &record, Phase::Activating, now_ms()?, None)?;
            }
            if record.phase == Phase::Activating {
                TlsIdentity::publish_staged_bundle(
                    &control.join("http-1"),
                    &record.activation_old_root()?,
                    record.new_root()?,
                    record
                        .activation_old_bundle
                        .as_ref()
                        .context("issuer activation predecessor missing")?,
                    &record.new_bundle,
                    true,
                    stopped.service_lease(),
                )?;
                http_endpoint::trust::commit_rotation_receipt(
                    &control.join("http-1"),
                    record.old_root()?,
                    record.new_root()?,
                    record.binding(),
                    record.old_ca_sha256,
                    record.new_ca_sha256,
                )?;
                record = rotation::advance(&control, &record, Phase::Restarting, now_ms()?, None)?;
            }
            let health = issuer_lifecycle::start_after_commit(stopped).await?;
            finish_started(install, &control, &record, &health).await?;
            output(&control, None)
        }
        HttpIssuerRotationAction::Finalize => {
            if record.phase == Phase::Finalized {
                return output(&control, None);
            }
            ensure!(
                record.phase == Phase::Activated
                    && now_ms()?
                        >= record
                            .overlap_expires_unix_ms
                            .context("issuer overlap unavailable")?,
                "issuer overlap has not expired or commit is incomplete"
            );
            rotation::require_start_allowed(&control)?;
            let old_root = record.old_root()?.to_path_buf();
            let binding = record.binding();
            let sha = record.old_ca_sha256;
            let result = tokio::task::spawn_blocking(move || {
                http_endpoint::trust::finalize_rotation_old(&old_root, binding, sha)
            })
            .await??;
            TlsIdentity::retire_staged_private_key(record.old_root()?, &record.old_bundle)?;
            TlsIdentity::retire_staged_private_key(
                &record.activation_old_root()?,
                record
                    .activation_old_bundle
                    .as_ref()
                    .context("issuer activation predecessor missing")?,
            )?;
            rotation::advance(&control, &record, Phase::Finalized, now_ms()?, None)?;
            output(&control, result.map(serde_json::to_value).transpose()?)
        }
    }
}

async fn finish_started(
    install: &Path,
    control: &Path,
    record: &Record,
    health: &ServiceHealth,
) -> Result<()> {
    ensure!(
        health.instance != record.old_endpoint.service_instance
            && health.executable_sha256 == record.old_endpoint.executable_sha256.to_string(),
        "issuer rotation replacement owner changed"
    );
    let endpoint = current_endpoint(install, control).await?;
    ensure!(
        endpoint.service_instance == health.instance,
        "issuer rotation health crossed service instance"
    );
    rotation::advance(control, record, Phase::Activated, now_ms()?, Some(endpoint))?;
    Ok(())
}
