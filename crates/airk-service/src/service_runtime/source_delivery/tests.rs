// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    identity::Incarnation, index_catalog::CatalogRevision, scoped_reads::SelectionOwnerKind,
};
#[tokio::test]
async fn actual_outer_write_checks_live_nonce_even_when_native_generation_did_not_change()
-> Result<()> {
    use tokio::io::AsyncReadExt;
    let service = Incarnation::from_bytes([1; 16]);
    let namespace = Incarnation::from_bytes([2; 16]);
    let gate = NativeCatalogAdmission::new(service, namespace, CatalogRevision::INITIAL, 1)?;
    let selection = SelectionScopeRef {
        owner: SelectionOwnerKind::Admin,
        grant_nonce: Incarnation::from_bytes([3; 16]),
        service_instance: service,
        scope_identity: Sha256Digest::from_bytes([4; 32]),
        catalog_revision: CatalogRevision::INITIAL,
    };
    gate.register_selection(&selection)?;
    let issue = gate.begin_issue(namespace, CatalogRevision::INITIAL)?;
    let context =
        ReadAttemptContext::new(Instant::now() + Duration::from_secs(5), Default::default());
    context.retain_source_delivery_guard(Arc::new(SourceDelivery {
        gate: gate.clone(),
        issue,
        selection: None,
        identity: Sha256Digest::from_bytes([5; 32]),
    }))?;
    context.retain_source_delivery_guard(Arc::new(SourceDelivery {
        gate: gate.clone(),
        issue,
        selection: Some(selection),
        identity: Sha256Digest::from_bytes([6; 32]),
    }))?;
    let (mut reader, mut raw) = tokio::io::duplex(1);
    let writing = tokio::spawn(async move {
        let mut writer = crate::service_runtime::policy_writer::PolicyWriter {
            inner: &mut raw,
            context: &context,
        };
        airk_ipc::endpoint::write_message(
            &mut writer,
            &ControlResponse::Failure {
                code: "synthetic-private-admin-body".into(),
            },
            Duration::from_secs(5),
        )
        .await
    });
    let mut prefix = [0u8; 1];
    reader.read_exact(&mut prefix).await?;
    let pending = gate.revoke_selection(&[selection.grant_nonce])?;
    assert!(pending.is_empty());
    gate.check_issue(issue)?; // A mere issue/clock probe would incorrectly pass.
    let mut remainder = Vec::new();
    reader.read_to_end(&mut remainder).await?;
    assert!(writing.await?.is_err());
    assert!(
        1 + remainder.len() < 4,
        "revoked native nonce must close a partial exchange without fallback"
    );
    Ok(())
}
