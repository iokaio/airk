// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::service_http::{EndpointState, LoopbackMcpUrl};

#[test]
fn expired_issuer_can_be_staged_from_verified_stopped_identity_without_renewal() -> Result<()> {
    let (_owner, control, endpoint, now) = fixture()?;
    let expired = now + 3651 * 86_400_000;
    let binding = installation_binding(&control, endpoint.namespace)?;
    let before = std::fs::read(control.join("http-1/tls-identity.json"))?;
    assert!(TlsIdentity::load_or_create(&control.join("http-1"), binding, expired).is_err());
    let _lease = airk_store::service::ServiceLease::acquire(&control)?;
    let record = prepare(
        &control,
        Incarnation::from_bytes([21; 16]),
        endpoint.clone(),
        PrepareReview {
            expected_ca: endpoint.ca_sha256,
            expected_descriptor: endpoint.digest()?,
            overlap_seconds: 60,
            now: expired,
            basis: PreparationBasis::StoppedNativeExclusion,
        },
    )?;
    assert!(record.old_ca_sha256 != record.new_ca_sha256);
    assert_eq!(
        std::fs::read(control.join("http-1/tls-identity.json"))?,
        before
    );
    assert!(matches!(
        record.preparation_basis,
        PreparationBasis::StoppedNativeExclusion
    ));
    Ok(())
}

#[test]
fn aborted_preparation_retains_original_issuer_and_terminal_public_receipt() -> Result<()> {
    let (_owner, control, endpoint, now) = fixture()?;
    let before = std::fs::read(control.join("http-1/tls-identity.json"))?;
    let record = prepare(
        &control,
        Incarnation::from_bytes([22; 16]),
        endpoint.clone(),
        PrepareReview {
            expected_ca: endpoint.ca_sha256,
            expected_descriptor: endpoint.digest()?,
            overlap_seconds: 60,
            now,
            basis: PreparationBasis::LiveNativeHealth,
        },
    )?;
    assert!(
        super::super::trust::abort_rotation_new(
            record.new_root()?,
            record.binding(),
            record.new_ca_sha256
        )?
        .is_none()
    );
    TlsIdentity::retire_staged_private_key(record.new_root()?, &record.new_bundle)?;
    TlsIdentity::retire_staged_private_key(record.old_root()?, &record.old_bundle)?;
    let aborted = advance(&control, &record, Phase::Aborted, now, None)?;
    require_start_allowed(&control)?;
    require_cleanup_allowed(&control)?;
    assert!(aborted.phase == Phase::Aborted);
    assert!(aborted.new_ca_certificate_file.is_file() && aborted.old_ca_certificate_file.is_file());
    assert_eq!(
        std::fs::read(control.join("http-1/tls-identity.json"))?,
        before
    );
    assert!(advance(&control, &aborted, Phase::Stopping, now, None).is_err());
    Ok(())
}

fn fixture() -> Result<(tempfile::TempDir, PathBuf, EndpointDescriptor, u64)> {
    let owner = tempfile::tempdir()?;
    let control = owner.path().join("control");
    crate::local_ipc::prepare_root(&control)?;
    let registry = airk_store::service::registry::ServiceRegistry::open(
        &control,
        Incarnation::from_bytes([7; 16]),
    )?;
    let binding = installation_binding(&control, registry.incarnation()?)?;
    drop(registry);
    let now = 1_800_000_000_000;
    let tls = TlsIdentity::load_or_create(&control.join("http-1"), binding.clone(), now)?;
    let endpoint = EndpointDescriptor {
        schema: 1,
        installation: binding.installation,
        user: binding.user.clone(),
        namespace: binding.namespace,
        generation: 1,
        service_instance: Incarnation::from_bytes([8; 16]),
        executable_sha256: Sha256Digest::from_bytes([9; 32]),
        url: LoopbackMcpUrl::new(57421)?,
        ca_sha256: tls.ca_sha256(),
        certificate_sha256: tls.certificate_sha256(),
        state: EndpointState::Ready,
        predecessor_digest: None,
    };
    publish_descriptor(&control.join("http-1"), &binding, &endpoint, None)?;
    Ok((owner, control, endpoint, now))
}

#[test]
fn preparation_is_review_bound_idempotent_and_does_not_replace_active_issuer() -> Result<()> {
    let (_owner, control, endpoint, now) = fixture()?;
    let operation = Incarnation::from_bytes([11; 16]);
    let before = std::fs::read(control.join("http-1/tls-identity.json"))?;
    let record = prepare(
        &control,
        operation,
        endpoint.clone(),
        PrepareReview {
            expected_ca: endpoint.ca_sha256,
            expected_descriptor: endpoint.digest()?,
            overlap_seconds: 60,
            now,
            basis: PreparationBasis::LiveNativeHealth,
        },
    )?;
    let again = prepare(
        &control,
        operation,
        endpoint.clone(),
        PrepareReview {
            expected_ca: endpoint.ca_sha256,
            expected_descriptor: endpoint.digest()?,
            overlap_seconds: 60,
            now: now + 10,
            basis: PreparationBasis::LiveNativeHealth,
        },
    )?;
    assert!(record.digest()? == again.digest()?);
    assert!(record.old_ca_sha256 != record.new_ca_sha256);
    assert_eq!(
        std::fs::read(control.join("http-1/tls-identity.json"))?,
        before
    );
    assert!(
        prepare(
            &control,
            operation,
            endpoint.clone(),
            PrepareReview {
                expected_ca: endpoint.ca_sha256,
                expected_descriptor: endpoint.digest()?,
                overlap_seconds: 61,
                now,
                basis: PreparationBasis::LiveNativeHealth
            }
        )
        .is_err()
    );
    assert!(reviewed(&control, operation, Sha256Digest::from_bytes([0; 32])).is_err());
    let public = serde_json::to_string(&record)?;
    assert!(
        !public.contains("PRIVATE KEY")
            && !public.contains("leaf_key")
            && !public.contains("ca_key")
    );
    assert!(require_cleanup_allowed(&control).is_err());
    Ok(())
}

#[test]
fn commit_requires_exclusion_and_expiry_then_old_issuer_cannot_restart() -> Result<()> {
    let (_owner, control, endpoint, now) = fixture()?;
    let operation = Incarnation::from_bytes([12; 16]);
    let prepared = prepare(
        &control,
        operation,
        endpoint.clone(),
        PrepareReview {
            expected_ca: endpoint.ca_sha256,
            expected_descriptor: endpoint.digest()?,
            overlap_seconds: 60,
            now,
            basis: PreparationBasis::LiveNativeHealth,
        },
    )?;
    let stopping = advance(&control, &prepared, Phase::Stopping, now, None)?;
    assert!(require_start_allowed(&control).is_err());
    let lease = airk_store::service::ServiceLease::acquire(&control)?;
    let current = TlsIdentity::snapshot_current_predecessor(
        &control.join("http-1"),
        stopping.old_root()?,
        &stopping.activation_old_root()?,
        &stopping.old_bundle,
        &lease,
    )?;
    let captured = record_activation_predecessor(
        &control,
        &stopping,
        current,
        serde_json::json!({"native_owner_exclusion":true}),
    )?;
    let activating = advance(&control, &captured, Phase::Activating, now, None)?;
    assert!(advance(&control, &prepared, Phase::Restarting, now, None).is_err());
    TlsIdentity::publish_staged_bundle(
        &control.join("http-1"),
        &activating.activation_old_root()?,
        activating.new_root()?,
        activating.activation_old_bundle.as_ref().unwrap(),
        &activating.new_bundle,
        true,
        &lease,
    )?;
    let restarting = advance(&control, &activating, Phase::Restarting, now, None)?;
    require_committed_restart(&control, operation, endpoint.executable_sha256)?;
    let mut next = endpoint.clone();
    next.generation += 1;
    next.service_instance = Incarnation::from_bytes([13; 16]);
    next.ca_sha256 = restarting.new_ca_sha256;
    next.certificate_sha256 = restarting.new_bundle.certificate_sha256;
    next.predecessor_digest = Some(endpoint.digest()?);
    let active = advance(&control, &restarting, Phase::Activated, now, Some(next))?;
    assert!(advance(&control, &active, Phase::Finalized, now + 59_999, None).is_err());
    let finalized = advance(&control, &active, Phase::Finalized, now + 60_000, None)?;
    assert!(finalized.phase == Phase::Finalized);
    require_cleanup_allowed(&control)?;
    TlsIdentity::publish_staged_bundle(
        &control.join("http-1"),
        &finalized.activation_old_root()?,
        finalized.new_root()?,
        finalized.activation_old_bundle.as_ref().unwrap(),
        &finalized.new_bundle,
        false,
        &lease,
    )?;
    assert!(require_start_allowed(&control).is_err());
    Ok(())
}
