// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    identity::Incarnation,
    service::RequestKey,
    service_lifecycle::{LifecycleAction, LifecyclePhase, LifecycleRequest},
};
use airk_store::service::{ServiceLease, registry::ServiceRegistry};

fn fixture() -> Result<(tempfile::TempDir, PathBuf, PathBuf, String)> {
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().canonicalize()?;
    let root = base.join("installation");
    local_ipc::prepare_root(&root)?;
    layout::initialize(
        &root,
        Some(&base.join("separate-state")),
        RuntimeChannel::Preview,
    )?;
    let source = base.join("approved-input");
    let identity = super::super::tests::fixture(&source)?;
    let slot = root.join("runtimes").join(&identity);
    local_ipc::prepare_root(&root.join("runtimes"))?;
    local_ipc::prepare_root(&slot)?;
    local_ipc::prepare_root(&slot.join("notices"))?;
    super::super::tests::fixture(&slot)?;
    let approval = Approval {
        schema: 1,
        manifest_sha256: identity.clone(),
        version: env!("CARGO_PKG_VERSION").into(),
        verification_basis: VerificationBasis::OrganizationDigest,
    };
    fs::write(slot.join("approval.json"), serde_json::to_vec(&approval)?)?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: identity.clone(),
            previous: None,
        },
    )?;
    let control = layout::control(&root)?;
    local_ipc::prepare_root(&control)?;
    let lease = ServiceLease::acquire(&control)?;
    layout::record_control(&lease)?;
    let instance = Incarnation::from_bytes([1; 16]);
    let mut registry = ServiceRegistry::open(&control, instance)?;
    registry.record_service_instance(&lease, instance)?;
    let (namespace, epoch) = registry.request_epoch()?;
    layout::record_registry(&lease, namespace)?;
    let request = LifecycleRequest {
        action: LifecycleAction::Stop,
        expected_instance: instance,
        caller: instance,
        key: RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([2; 16]),
        },
        runtime: identity.parse()?,
        manager: false,
    };
    let request = registry.begin_lifecycle(&request)?;
    registry.advance_lifecycle(&request, LifecyclePhase::Complete)?;
    Ok((temporary, root, source, identity))
}
fn reviewed(root: &Path, source: &Path, identity: &str) -> Result<Sha256Digest> {
    preview(root, source, identity)?["preview"]
        .as_str()
        .context("preview missing")?
        .parse()
}
fn replace_fixture(_: &Path, root: &Path, approval: &Approval) -> Result<Approval> {
    let slot = root.join("runtimes").join(&approval.manifest_sha256);
    local_ipc::prepare_root(&slot)?;
    local_ipc::prepare_root(&slot.join("notices"))?;
    ensure!(
        super::super::tests::fixture(&slot)? == approval.manifest_sha256,
        "fixture changed"
    );
    fs::write(slot.join("approval.json"), serde_json::to_vec(approval)?)?;
    Ok(approval.clone())
}

#[tokio::test]
async fn repair_preserves_modified_and_unknown_files_and_resumes_after_copy_failure() -> Result<()>
{
    let (_temporary, root, source, identity) = fixture()?;
    let slot = root.join("runtimes").join(&identity);
    let original = b"User-modified original bytes, retained exactly.";
    fs::write(slot.join(main_name()), original)?;
    fs::remove_file(slot.join(helper_name()))?;
    fs::write(
        slot.join("unknown-note.txt"),
        b"Preserve this unknown original.",
    )?;
    let selected_before = fs::read(root.join("selected.json"))?;
    let token = reviewed(&root, &source, &identity)?;
    assert!(
        apply_with(
            &root,
            &source,
            &identity,
            Sha256Digest::from_bytes([0; 32]),
            replace_fixture
        )
        .is_err()
    );
    assert!(!root.join(INTENT).exists());
    assert_eq!(fs::read(slot.join(main_name()))?, original);
    assert!(
        apply_with(&root, &source, &identity, token, |_, _, _| bail!(
            "injected copy failure"
        ))
        .is_err()
    );
    assert!(root.join(INTENT).is_file());
    let preserved = backup(&root, &identity)?;
    assert_eq!(fs::read(preserved.join(main_name()))?, original);
    assert!(!slot.exists());
    let saved = snapshot::read(&root, None, None, 16)?;
    assert_eq!(saved["selection"]["active"], identity);
    assert_eq!(saved["lifecycle"]["explicitly_stopped"], true);
    assert!(selected(&root).is_err());
    assert_eq!(reviewed(&root, &source, &identity)?, token);
    fs::write(
        preserved.join("unknown-note.txt"),
        b"Changed during interruption.",
    )?;
    assert!(reviewed(&root, &source, &identity).is_err());
    assert!(apply_with(&root, &source, &identity, token, replace_fixture).is_err());
    fs::write(
        preserved.join("unknown-note.txt"),
        b"Preserve this unknown original.",
    )?;
    assert_eq!(
        apply_with(&root, &source, &identity, token, replace_fixture)?["repaired"],
        true
    );
    assert!(!root.join(INTENT).exists());
    assert_eq!(selected(&root)?.0.active, identity);
    assert_eq!(
        apply_with(&root, &source, &identity, token, |_, _, _| bail!(
            "replay must not copy again"
        ))?["replayed"],
        true
    );
    assert_eq!(fs::read(preserved.join(main_name()))?, original);
    assert_eq!(
        fs::read(preserved.join("unknown-note.txt"))?,
        b"Preserve this unknown original."
    );
    assert_eq!(fs::read(root.join("selected.json"))?, selected_before);
    assert_eq!(lifecycle::inspect(&root)?["explicitly_stopped"], true);
    assert_eq!(
        layout::retained_parameters(&root)?.1,
        RuntimeChannel::Preview
    );
    assert!(
        apply_with(
            &root,
            &source,
            &identity,
            reviewed(&root, &source, &identity)?,
            replace_fixture
        )
        .is_err()
    );
    assert_eq!(retirement::uninstall(&root).await?["uninstalled"], true);
    assert_eq!(fs::read(preserved.join(main_name()))?, original);
    assert!(!slot.exists());
    Ok(())
}

#[test]
fn repair_requires_quiescence_and_refuses_linked_or_changed_inputs() -> Result<()> {
    let (_temporary, root, source, identity) = fixture()?;
    let token = reviewed(&root, &source, &identity)?;
    let lease = ServiceLease::acquire(&layout::control(&root)?)?;
    assert!(apply_with(&root, &source, &identity, token, replace_fixture).is_err());
    assert!(!root.join(INTENT).exists());
    drop(lease);
    let slot = root.join("runtimes").join(&identity);
    let link = slot.join("linked-main");
    fs::hard_link(slot.join(main_name()), &link)?;
    assert!(reviewed(&root, &source, &identity).is_err());
    fs::remove_file(link)?;
    fs::write(source.join(main_name()), b"Changed approved input.")?;
    assert!(preview(&root, &source, &identity).is_err());
    assert!(apply_with(&root, &source, &identity, token, replace_fixture).is_err());
    assert!(!root.join(INTENT).exists());
    Ok(())
}
