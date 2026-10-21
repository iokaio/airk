// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_store::current::{CapturedInputs, TransactionPhase};

#[test]
fn ready_recovery_releases_failed_prepare_allowance_only_after_lexical_verification() -> Result<()>
{
    struct Failure;
    impl CapturedInputs for Failure {
        fn visit(&self, _: &mut dyn FnMut(&CapturedInput) -> Result<()>) -> Result<()> {
            anyhow::bail!("injected failed capture after allowance")
        }
    }
    let fixture = Fixture::new()?;
    let (_reservation, head, overlay) = fixture.candidate()?;
    let sealed = Fixture::seal(overlay)?;
    let mut target = head.clone();
    target.sequence += 1;
    target.graph_head = sealed.descriptor().target.clone();
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let error = current
        .prepare(&head.token()?, target, &sealed, &Failure, None)
        .unwrap_err();
    assert!(error.to_string().contains("injected failed capture"));
    assert_eq!(current.phase()?, TransactionPhase::Ready);
    assert!(current.pending()?.is_none());
    assert_eq!(fixture.quota.current_grants(&writer)?.len(), 1);
    drop(current);
    drop(writer);
    let lexical_meta = fixture.store.directory().join("lexical-0/meta.json");
    let original = std::fs::read(&lexical_meta)?;
    std::fs::write(&lexical_meta, b"damaged fixture metadata")?;
    assert!(
        airk_kernel::current::recover_current(&fixture.store, &fixture.quota, &fixture.scope)
            .is_err()
    );
    let writer = fixture.store.lock()?;
    assert_eq!(fixture.quota.current_grants(&writer)?.len(), 1);
    drop(writer);
    std::fs::write(&lexical_meta, original)?;
    let recovered =
        airk_kernel::current::recover_current(&fixture.store, &fixture.quota, &fixture.scope)?;
    assert_eq!(
        recovered.action,
        airk_kernel::current::CurrentRecoveryAction::AlreadyReady
    );
    assert_eq!(recovered.token, head.token()?);
    assert_eq!(fixture.store.current_read(&fixture.scope)?.head(), &head);
    let writer = fixture.store.lock()?;
    assert!(fixture.quota.current_grants(&writer)?.is_empty());
    Ok(())
}
