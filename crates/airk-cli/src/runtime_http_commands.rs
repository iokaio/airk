// SPDX-License-Identifier: Apache-2.0
//! Setup-only HTTP discovery and enrollment through the verified native service.
use super::*;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service_http::{EndpointDescriptor, HttpSelection},
};

#[cfg(test)]
#[path = "runtime_http_commands_tests.rs"]
mod tests;

async fn control(install: &Path) -> Result<(PathBuf, ServiceHealth)> {
    let control = installed_control(install).await?;
    let response = service_runtime::request(&control, ControlMethod::Health).await?;
    let ControlResponse::Health { observation } = response else {
        return refused(response);
    };
    ensure!(observation.policy.accepting, "native service is draining");
    Ok((control, *observation))
}
fn refused<T>(response: ControlResponse) -> Result<T> {
    // The canonical native refusal remains complete in the diagnostic. No
    // credential bytes are ever emitted by these commands or control responses.
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
    let binding = crate::http_endpoint::installation_binding(control, endpoint.namespace)?;
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
fn same_authority(requested: &HttpSelection, observed: &HttpSelection) -> bool {
    let mut normalized = requested.clone();
    match (&mut normalized, observed) {
        (HttpSelection::Scoped { selection: a }, HttpSelection::Scoped { selection: b })
            if b.expected_catalog_revision >= a.expected_catalog_revision =>
        {
            a.expected_catalog_revision = b.expected_catalog_revision
        }
        (HttpSelection::Combined { selection: a }, HttpSelection::Combined { selection: b })
            if b.expected_catalog_revision >= a.expected_catalog_revision =>
        {
            a.expected_catalog_revision = b.expected_catalog_revision
        }
        (
            HttpSelection::ImportedLegacy { selection: a },
            HttpSelection::ImportedLegacy { selection: b },
        ) if b.expected_catalog_revision >= a.expected_catalog_revision => {
            a.expected_catalog_revision = b.expected_catalog_revision
        }
        (
            HttpSelection::ImportedLegacyUnion { selection: a },
            HttpSelection::ImportedLegacyUnion { selection: b },
        ) if a.selections.len() == b.selections.len() => {
            for (left, right) in a.selections.iter_mut().zip(&b.selections) {
                if right.expected_catalog_revision < left.expected_catalog_revision {
                    return false;
                }
                left.expected_catalog_revision = right.expected_catalog_revision;
            }
        }
        _ => return false,
    }
    normalized == *observed
}
fn connection(
    response: ControlResponse,
    control: &Path,
    health: &ServiceHealth,
    id: Incarnation,
    selection: &HttpSelection,
) -> Result<serde_json::Value> {
    let ControlResponse::HttpConnection { connection } = response else {
        return refused(response);
    };
    connection.validate()?;
    check_endpoint(control, &connection.endpoint, health)?;
    ensure!(
        connection.connection_id == id && same_authority(selection, &connection.selection),
        "native HTTP enrollment selection changed"
    );
    let private = control.join("http-1");
    ensure!(
        Path::new(&connection.ca_certificate_file) == private.join("ca.pem")
            && Path::new(&connection.credential_file)
                == private.join(format!("credential-{id}.token")),
        "native HTTP credential references changed"
    );
    Ok(serde_json::to_value(connection)?)
}
pub async fn endpoint(install: &Path) -> Result<serde_json::Value> {
    let (control, health) = control(install).await?;
    let response = service_runtime::request_for_instance(
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
/// Resolve only an existing native credential. The secret stays inside this
/// process; callers receive the same protected references as enrollment.
pub async fn existing_connection(install: &Path, id: Incarnation) -> Result<serde_json::Value> {
    let (control, health) = control(install).await?;
    let response = service_runtime::request_for_instance(
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
    resolved_existing_connection(control, health, *endpoint, id).await
}

async fn resolved_existing_connection(
    control: PathBuf,
    health: ServiceHealth,
    endpoint: EndpointDescriptor,
    id: Incarnation,
) -> Result<serde_json::Value> {
    check_endpoint(&control, &endpoint, &health)?;
    let binding = crate::http_endpoint::installation_binding(&control, endpoint.namespace)?;
    let private = control.join("http-1");
    let token_root = private.clone();
    let authenticated = tokio::task::spawn_blocking(move || -> Result<_> {
        let token = crate::http_endpoint::read_client_token(&token_root, id)?;
        let store = crate::http_endpoint::CredentialStore::read_existing(&token_root, binding)?;
        let now: u64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
            .try_into()?;
        store.authenticate(&token, now)
    })
    .await??;
    ensure!(
        authenticated.connection_id == id,
        "native HTTP connection identity changed"
    );
    let context = airk_mcp::read_attempt::ReadAttemptContext::new(
        std::time::Instant::now() + std::time::Duration::from_secs(30),
        tokio_util::sync::CancellationToken::new(),
    );
    let selection =
        crate::http_backend::validate(&control, &authenticated.selection, &context).await?;
    let connection = airk_core::service_http::HttpConnectionInfo {
        schema: airk_core::service_http::HTTP_CONTRACT_VERSION,
        connection_id: id,
        endpoint,
        selection,
        ca_certificate_file: private
            .join("ca.pem")
            .to_str()
            .context("HTTP CA path not UTF-8")?
            .into(),
        credential_file: private
            .join(format!("credential-{id}.token"))
            .to_str()
            .context("HTTP credential path not UTF-8")?
            .into(),
    };
    connection.validate()?;
    Ok(serde_json::to_value(connection)?)
}

/// Typed configuration ownership lookup. It never starts a service, enrolls a
/// credential, mutates a selector or treats a revoked/missing secret as foreign.
pub async fn lookup_connection(
    install: &Path,
    expected: &str,
    id: Incarnation,
) -> Result<serde_json::Value> {
    use crate::http_endpoint::{ConnectionOwnership, CredentialStore};
    let _lock = existing_install_lock(install)?;
    ensure!(
        selection_record(install)?.active == expected,
        "HTTP lookup selected runtime changed"
    );
    let (main, approval) = verify_runtime(install, expected)?;
    let control = layout::saved_control(install)?;
    local_ipc::require_existing_root(&control)?;
    let ControlResponse::Health {
        observation: health,
    } = service_runtime::request_for_version(&control, ControlMethod::Health, &approval.version)
        .await?
    else {
        bail!("HTTP lookup native health unavailable");
    };
    ensure!(
        health.runtime_sha256.as_deref() == Some(expected)
            && health.executable_sha256 == runtime_pair::digest_file(&main)?
            && health.policy.accepting,
        "HTTP lookup native owner is unavailable or changed"
    );
    let response = service_runtime::request_for_instance(
        &control,
        ControlMethod::HttpEndpoint,
        health.instance,
    )
    .await?;
    let ControlResponse::HttpEndpoint { endpoint } = response else {
        return refused(response);
    };
    let endpoint = endpoint.context("HTTP lookup endpoint unavailable")?;
    check_endpoint(&control, &endpoint, &health)?;
    let binding = crate::http_endpoint::installation_binding(&control, endpoint.namespace)?;
    let private = control.join("http-1");
    let (ownership, attachment_retirement) = tokio::task::spawn_blocking(move || -> Result<_> {
        let store = CredentialStore::read_existing(&private, binding)?;
        let now: u64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
            .try_into()?;
        Ok((
            store.ownership(id, now)?,
            store.attachment_retirement(id, now)?,
        ))
    })
    .await??;
    let mut reply = serde_json::json!({"schema":1,"active":expected,"connection_id":id});
    if let Some(proof) = attachment_retirement {
        reply["status"] = "unavailable".into();
        reply["code"] = "owned_connection_attachment_retired".into();
        reply["attachment_retirement"] = serde_json::to_value(proof)?;
        reply["guidance"] = "Only the exact original managed standalone claim may replace this retired attachment using a fresh ID and native mutation-time retirement validation.".into();
        return Ok(reply);
    }
    match ownership {
        ConnectionOwnership::NotOwned => reply["status"] = "not_owned".into(),
        ConnectionOwnership::Revoked | ConnectionOwnership::Expired => {
            reply["status"] = "unavailable".into();
            reply["code"] = if ownership == ConnectionOwnership::Revoked {
                "owned_connection_revoked"
            } else {
                "owned_connection_expired"
            }
            .into();
            reply["guidance"] = "This installation owns the ID. Retain its configuration and review renewal or revocation explicitly; do not reenroll it during migration.".into();
        }
        ConnectionOwnership::Active => {
            match resolved_existing_connection(control, *health, *endpoint, id).await {
                Ok(connection) => {
                    reply["status"] = "owned".into();
                    reply["connection"] = connection;
                }
                Err(error) => {
                    reply["status"] = "unavailable".into();
                    reply["code"] = "owned_connection_unavailable".into();
                    reply["native_error"] = format!("{error:#}").into();
                    reply["guidance"] = "The owned connection could not be authenticated or its live grant observed. Retain the exact configuration and diagnose the native refusal; no credential was issued.".into();
                }
            }
        }
    }
    Ok(reply)
}
fn client(label: &str) -> Result<airk_core::usage::ClientFamily> {
    ensure!(
        !label.is_empty()
            && label.len() <= 64
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte)),
        "invalid HTTP client label"
    );
    Ok(airk_core::usage::ClientFamily::from_label(label))
}
async fn enroll_at(
    control: &Path,
    health: &ServiceHealth,
    selection: HttpSelection,
    id: Incarnation,
    client: airk_core::usage::ClientFamily,
    explorer_descriptor: Option<&Path>,
    reattach_connection_id: Option<Incarnation>,
) -> Result<serde_json::Value> {
    selection.validate()?;
    let explorer_descriptor = explorer_descriptor
        .map(|path| -> Result<String> {
            ensure!(path.is_absolute(), "Explorer descriptor must be absolute");
            Ok(path
                .to_str()
                .context("Explorer descriptor path is not UTF-8")?
                .to_owned())
        })
        .transpose()?;
    let response = service_runtime::request_for_instance(
        control,
        ControlMethod::HttpEnroll {
            connection_id: id,
            selection: Box::new(selection.clone()),
            client,
            explorer_descriptor,
            reattach_connection_id,
        },
        health.instance,
    )
    .await?;
    connection(response, control, health, id, &selection)
}
pub async fn enroll(
    install: &Path,
    file: &Path,
    id: Incarnation,
    label: &str,
    explorer_descriptor: Option<&Path>,
) -> Result<serde_json::Value> {
    let selection = crate::http_endpoint::read_selection_file(file)?;
    let client = client(label)?;
    let (control, health) = control(install).await?;
    enroll_at(
        &control,
        &health,
        selection,
        id,
        client,
        explorer_descriptor,
        None,
    )
    .await
}
/// CLI setup supplies an explicit existing selector, never workspace authority.
pub async fn setup(
    install: Option<&Path>,
    runtime_control: Option<&Path>,
    selection: HttpSelection,
    id: Incarnation,
    label: &str,
    explorer_descriptor: Option<&Path>,
) -> Result<serde_json::Value> {
    let client = client(label)?;
    let (control, health) = if let Some(saved) = runtime_control {
        let saved = saved.canonicalize()?;
        validate_control_root(&saved)?;
        let response = service_runtime::request(&saved, ControlMethod::Health).await?;
        let ControlResponse::Health { observation } = response else {
            return refused(response);
        };
        ensure!(observation.policy.accepting, "native service is draining");
        (saved, *observation)
    } else {
        let install = install
            .map(Path::to_path_buf)
            .map(Ok)
            .unwrap_or_else(caller_install_root)?;
        control(&install).await?
    };
    enroll_at(
        &control,
        &health,
        selection,
        id,
        client,
        explorer_descriptor,
        None,
    )
    .await
}

pub async fn revoke(install: &Path, id: Incarnation) -> Result<serde_json::Value> {
    let (control, health) = control(install).await?;
    let response = service_runtime::request_for_instance(
        &control,
        ControlMethod::HttpRevoke { connection_id: id },
        health.instance,
    )
    .await?;
    match &response {
        ControlResponse::HttpRevoked { connection_id } if *connection_id == id => {
            Ok(serde_json::to_value(response)?)
        }
        _ => refused(response),
    }
}
#[allow(clippy::too_many_arguments)]
pub async fn enroll_legacy(
    install: &Path,
    file: &Path,
    receipt: Sha256Digest,
    id: Incarnation,
    label: &str,
    explorer_descriptor: Option<&Path>,
    origin_sha256: Option<Sha256Digest>,
    reattach_connection_id: Option<Incarnation>,
) -> Result<serde_json::Value> {
    let registration = crate::http_endpoint::read_registration_file(file)?;
    let client = client(label)?;
    let (control, health) = control(install).await?;
    let resolution = {
        let mut registry = airk_store::service::registry::ServiceRegistry::open_saved_before(
            &control,
            Some(std::time::Instant::now() + Duration::from_secs(2)),
        )?;
        http_legacy::resolve_claim(&mut registry, registration, receipt, origin_sha256)?
    };
    match resolution {
        http_legacy::LegacyMigrationResolution::Eligible { selection } => {
            enroll_at(
                &control,
                &health,
                HttpSelection::ImportedLegacy { selection },
                id,
                client,
                explorer_descriptor,
                reattach_connection_id,
            )
            .await
        }
        http_legacy::LegacyMigrationResolution::ReviewRequired {
            registration,
            configuration_receipt,
        } => Ok(
            serde_json::json!({"schema":1,"status":"review_required","code":"legacy_selection_review_required","registration":registration,"configuration_receipt":configuration_receipt}),
        ),
    }
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
            let (binding, endpoint) = crate::http_endpoint::trust::saved_identity(&control)?;
            check_reviewed_ca(action, expected_ca, endpoint.ca_sha256)?;
            crate::http_endpoint::trust::manage(
                &control.join("http-1"),
                binding,
                endpoint.ca_sha256,
                if action == HttpTrustAction::Remove {
                    crate::http_endpoint::trust::Action::Remove
                } else {
                    crate::http_endpoint::trust::Action::Inspect
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
    let response = service_runtime::request(&control, ControlMethod::Health).await?;
    let ControlResponse::Health { observation } = response else {
        return refused(response);
    };
    require_matching_health(&approval, &observation)?;
    let response = service_runtime::request_for_instance(
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
    let binding = crate::http_endpoint::installation_binding(&control, endpoint.namespace)?;
    let action = match action {
        HttpTrustAction::Inspect => crate::http_endpoint::trust::Action::Inspect,
        HttpTrustAction::Install => crate::http_endpoint::trust::Action::Install,
        HttpTrustAction::Remove => crate::http_endpoint::trust::Action::Remove,
    };
    let root = control.join("http-1");
    let status = tokio::task::spawn_blocking(move || {
        crate::http_endpoint::trust::manage(&root, binding, endpoint.ca_sha256, action)
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
pub async fn enroll_legacy_union(
    install: &Path,
    file: &Path,
    id: Incarnation,
    label: &str,
    explorer_descriptor: Option<&Path>,
) -> Result<serde_json::Value> {
    let input = crate::http_endpoint::read_registrations_file(file)?;
    let client = client(label)?;
    let (control, health) = control(install).await?;
    let outcomes = {
        let mut registry = airk_store::service::registry::ServiceRegistry::open_saved_before(
            &control,
            Some(std::time::Instant::now() + Duration::from_secs(2)),
        )?;
        input
            .registrations
            .into_iter()
            .map(|member| {
                http_legacy::resolve(
                    &mut registry,
                    member.registration,
                    member.configuration_receipt,
                )
            })
            .collect::<Result<Vec<_>>>()?
    };
    if outcomes.iter().any(|member| {
        matches!(
            member,
            http_legacy::LegacyMigrationResolution::ReviewRequired { .. }
        )
    }) {
        return Ok(
            serde_json::json!({"schema":1,"status":"review_required","code":"legacy_union_review_required","no_default_project":true,"resolutions":outcomes}),
        );
    }
    let selections = outcomes
        .into_iter()
        .map(|member| match member {
            http_legacy::LegacyMigrationResolution::Eligible { selection } => Ok(*selection),
            _ => bail!("legacy union resolution changed"),
        })
        .collect::<Result<Vec<_>>>()?;
    let namespace = selections
        .first()
        .context("legacy union is empty")?
        .namespace;
    let selection = HttpSelection::ImportedLegacyUnion {
        selection: airk_core::service_http::NativeImportedHttpUnion {
            namespace,
            selections,
            no_default_project: true,
        },
    };
    selection.validate()?;
    enroll_at(
        &control,
        &health,
        selection,
        id,
        client,
        explorer_descriptor,
        None,
    )
    .await
}
