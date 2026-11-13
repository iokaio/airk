// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn interrupted_rotation_requires_the_exact_recorded_native_service_owner() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    let control = fixture.path().canonicalize()?.join("control");
    local_ipc::prepare_root(&control)?;
    let lease = airk_store::service::ServiceLease::acquire(&control)?;
    let original = Incarnation::from_bytes([31; 16]);
    let replacement = Incarnation::from_bytes([32; 16]);
    let mut registry = airk_store::service::registry::ServiceRegistry::open(&control, original)?;
    assert!(
        require_recorded_owner(&control, original).is_err(),
        "missing saved owner is not exit proof"
    );
    registry.record_service_instance(&lease, original)?;
    require_recorded_owner(&control, original)?;
    assert!(require_recorded_owner(&control, replacement).is_err());
    registry.record_service_instance(&lease, replacement)?;
    assert!(
        require_recorded_owner(&control, original).is_err(),
        "an intervening owner cannot be treated as the original interrupted service"
    );
    require_recorded_owner(&control, replacement)?;
    Ok(())
}

#[test]
fn committed_successor_requires_its_own_endpoint_image_and_actual_native_exclusion() -> Result<()> {
    use airk_core::service_http::{EndpointDescriptor, EndpointState, LoopbackMcpUrl};
    use airk_endpoint::http_endpoint::{self as http, TlsIdentity, rotation};
    let fixture = tempfile::tempdir()?;
    let control = fixture.path().canonicalize()?.join("control");
    local_ipc::prepare_root(&control)?;
    let lease = airk_store::service::ServiceLease::acquire(&control)?;
    let original = Incarnation::from_bytes([41; 16]);
    let replacement = Incarnation::from_bytes([42; 16]);
    let mut registry = airk_store::service::registry::ServiceRegistry::open(&control, original)?;
    registry.record_service_instance(&lease, original)?;
    let binding = http::installation_binding(&control, registry.incarnation()?)?;
    let active = control.join("http-1");
    let now = 1_800_000_000_000;
    let tls = TlsIdentity::load_or_create(&active, binding.clone(), now)?;
    let endpoint = EndpointDescriptor {
        schema: 1,
        installation: binding.installation,
        user: binding.user.clone(),
        namespace: binding.namespace,
        generation: 1,
        service_instance: original,
        executable_sha256: Sha256Digest::from_bytes([43; 32]),
        url: LoopbackMcpUrl::new(57421)?,
        ca_sha256: tls.ca_sha256(),
        certificate_sha256: tls.certificate_sha256(),
        state: EndpointState::Ready,
        predecessor_digest: None,
    };
    http::publish_descriptor(&active, &binding, &endpoint, None)?;
    let prepared = rotation::prepare(
        &control,
        Incarnation::from_bytes([44; 16]),
        endpoint.clone(),
        rotation::PrepareReview {
            expected_ca: endpoint.ca_sha256,
            expected_descriptor: endpoint.digest()?,
            overlap_seconds: 60,
            now,
            basis: rotation::PreparationBasis::LiveNativeHealth,
        },
    )?;
    let stopping = rotation::advance(&control, &prepared, rotation::Phase::Stopping, now, None)?;
    let current = TlsIdentity::snapshot_current_predecessor(
        &active,
        stopping.old_root()?,
        &stopping.activation_old_root()?,
        &stopping.old_bundle,
        &lease,
    )?;
    let captured = rotation::record_activation_predecessor(
        &control,
        &stopping,
        current,
        serde_json::json!({"test_only":"inert original stop proof"}),
    )?;
    let activating =
        rotation::advance(&control, &captured, rotation::Phase::Activating, now, None)?;
    TlsIdentity::publish_staged_bundle(
        &active,
        &activating.activation_old_root()?,
        activating.new_root()?,
        activating.activation_old_bundle.as_ref().unwrap(),
        &activating.new_bundle,
        true,
        &lease,
    )?;
    let restarting = rotation::advance(
        &control,
        &activating,
        rotation::Phase::Restarting,
        now,
        None,
    )?;
    let digest = restarting.digest()?;
    assert_eq!(committed_successor(&control, &restarting, &lease)?, None);
    registry.record_service_instance(&lease, replacement)?;
    assert!(
        committed_successor(&control, &restarting, &lease).is_err(),
        "recorded replacement before endpoint publication is not approved-image proof"
    );
    let mut successor = endpoint.clone();
    successor.service_instance = replacement;
    successor.ca_sha256 = restarting.new_ca_sha256;
    successor.certificate_sha256 = restarting.new_bundle.certificate_sha256;
    successor.generation += 1;
    successor.predecessor_digest = Some(endpoint.digest()?);
    http::publish_descriptor(&active, &binding, &successor, successor.predecessor_digest)?;
    assert_eq!(
        committed_successor(&control, &restarting, &lease)?,
        Some(replacement)
    );
    assert!(
        require_recorded_owner(&control, original).is_err(),
        "successor must never be relabeled as OLD"
    );
    let mut foreign = successor.clone();
    foreign.executable_sha256 = Sha256Digest::from_bytes([45; 32]);
    foreign.generation += 1;
    foreign.predecessor_digest = Some(successor.digest()?);
    http::publish_descriptor(&active, &binding, &foreign, foreign.predecessor_digest)?;
    assert!(committed_successor(&control, &restarting, &lease).is_err());
    assert!(
        rotation::read(&control)?.unwrap().digest()? == digest,
        "original rotation/stop proof must remain immutable"
    );
    Ok(())
}
