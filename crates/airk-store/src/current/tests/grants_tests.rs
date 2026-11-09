// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::quota::CurrentGrantScope;

struct Failure;
impl CapturedInputs for Failure {
    fn visit(&self, _: &mut dyn FnMut(&CapturedInput) -> Result<()>) -> Result<()> {
        anyhow::bail!("injected capture failure after grant admission")
    }
}

fn grants(quota: &Quota, writer: &crate::WriterLock) -> Result<Vec<CurrentGrantScope>> {
    let mut scopes = quota
        .current_grants(writer)?
        .into_iter()
        .map(|grant| grant.scope().clone())
        .collect::<Vec<_>>();
    scopes.sort_by_key(|scope| (scope.incarnation, scope.transaction));
    Ok(scopes)
}

#[test]
fn failed_prepare_retries_same_transaction_without_orphan_grant_collision() -> Result<()> {
    let fixture = Fixture::new()?;
    let (_reservation, sealed) = fixture.sealed()?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let error = current
        .prepare(
            &fixture.head.token()?,
            fixture.target(&sealed),
            &sealed,
            &Failure,
            None,
        )
        .unwrap_err();
    assert!(error.to_string().contains("injected capture failure"));
    assert_eq!(current.phase()?, TransactionPhase::Ready);
    assert!(current.pending()?.is_none());
    assert_eq!(current.head()?, fixture.head);
    let orphan = grants(&fixture.quota, &writer)?;
    assert_eq!(orphan.len(), 1);
    let pending = current.prepare(
        &fixture.head.token()?,
        fixture.target(&sealed),
        &sealed,
        &[],
        None,
    )?;
    assert_eq!(pending.id, orphan[0].transaction);
    assert_eq!(grants(&fixture.quota, &writer)?.len(), 1);
    current.mark_applying(&pending)?;
    current.commit(&pending, pending.target.lexical_commit)?;
    assert!(grants(&fixture.quota, &writer)?.is_empty());
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert_eq!(read.token()?, pending.target.token()?);
    assert_eq!(read.read(1)?.bytes, vec![7; 16]);
    Ok(())
}

#[test]
fn orphan_cleanup_preserves_all_grants_while_prepared_or_applying() -> Result<()> {
    for applying in [false, true] {
        let fixture = Fixture::new()?;
        let pending = fixture.prepare(&[])?;
        let writer = fixture.store.lock()?;
        let mut current =
            fixture
                .store
                .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
        if applying {
            current.mark_applying(&pending)?;
        }
        fixture.quota.reserve_current(
            &writer,
            CurrentGrantScope {
                collection: fixture.scope.collection.clone(),
                incarnation: Incarnation::from_bytes([93; 16]),
                transaction: Sha256Digest::from_bytes([94; 32]),
            },
            4096,
        )?;
        let before = grants(&fixture.quota, &writer)?;
        assert_eq!(before.len(), 2);
        let error = current.release_orphan_grants().unwrap_err();
        assert!(error.to_string().contains("still owns recovery capacity"));
        assert_eq!(current.pending()?, Some(pending));
        assert_eq!(grants(&fixture.quota, &writer)?, before);
        assert_eq!(
            current.phase()?,
            if applying {
                TransactionPhase::Applying
            } else {
                TransactionPhase::Prepared
            }
        );
    }
    Ok(())
}

#[test]
fn orphan_cleanup_validates_foreign_incarnations_before_releasing_any_grant() -> Result<()> {
    let fixture = Fixture::new()?;
    let writer = fixture.store.lock()?;
    let current = fixture
        .store
        .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    for id in 10..18 {
        fixture.quota.reserve_current(
            &writer,
            CurrentGrantScope {
                collection: fixture.scope.collection.clone(),
                incarnation: fixture.scope.incarnation,
                transaction: Sha256Digest::from_bytes([id; 32]),
            },
            4096,
        )?;
    }
    fixture.quota.reserve_current(
        &writer,
        CurrentGrantScope {
            collection: fixture.scope.collection.clone(),
            incarnation: Incarnation::from_bytes([93; 16]),
            transaction: Sha256Digest::from_bytes([94; 32]),
        },
        4096,
    )?;
    let before = grants(&fixture.quota, &writer)?;
    let error = current.release_orphan_grants().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("foreign current recovery capacity grant")
    );
    assert_eq!(grants(&fixture.quota, &writer)?, before);
    assert_eq!(current.head()?, fixture.head);
    assert!(current.pending()?.is_none());
    Ok(())
}

#[test]
fn orphan_cleanup_never_releases_another_collections_allowance() -> Result<()> {
    let fixture = Fixture::new()?;
    let other = FileStore::new(fixture.quota.root().join("collections-1.2/other"));
    let other_writer = other.lock()?;
    let other_scope = CurrentGrantScope {
        collection: "other".into(),
        incarnation: Incarnation::from_bytes([93; 16]),
        transaction: Sha256Digest::from_bytes([94; 32]),
    };
    fixture
        .quota
        .reserve_current(&other_writer, other_scope.clone(), 4096)?;
    let writer = fixture.store.lock()?;
    let current = fixture
        .store
        .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    fixture.quota.reserve_current(
        &writer,
        CurrentGrantScope {
            collection: fixture.scope.collection.clone(),
            incarnation: fixture.scope.incarnation,
            transaction: Sha256Digest::from_bytes([95; 32]),
        },
        4096,
    )?;
    assert_eq!(current.release_orphan_grants()?, 1);
    assert!(grants(&fixture.quota, &writer)?.is_empty());
    assert_eq!(grants(&fixture.quota, &other_writer)?, [other_scope]);
    Ok(())
}
