// SPDX-License-Identifier: Apache-2.0
//! Real data binding, collection/writer/quota producers and native exclusions.
use super::*;
#[path = "service_data_scope_clean_break_replay_tests.rs"]
mod replay;
use crate::{FileStore, quota::Quota, service::collection::Collection};

struct Fixture {
    _owner: tempfile::TempDir,
    data: PathBuf,
    source: PathBuf,
    service: ServiceLease,
    namespace: Incarnation,
    registrations: Vec<ProjectRegistration>,
}
impl Fixture {
    fn new() -> Result<Self> {
        let owner = tempfile::tempdir()?;
        let base = owner.path().canonicalize()?;
        let data = base.join("data");
        let control = base.join("control");
        let source = base.join("source");
        for directory in [&data, &control, &source] {
            fs::create_dir(directory)?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Existing storage privacy is advisory; a reset must not impose the
            // separate IPC credential-directory permission contract on it.
            fs::set_permissions(&data, fs::Permissions::from_mode(0o755))?;
        }
        fs::write(source.join("original.txt"), b"original source")?;
        fs::write(base.join("foreign-sibling.txt"), b"unrelated sibling")?;
        let namespace = Incarnation::from_bytes([91; 16]);
        let service = ServiceLease::acquire(&control)?;
        let mut registry = ServiceRegistry::open(&control, namespace)?;
        bind(&data, &control, namespace)?;
        let mut registrations = Vec::new();
        for (index, name) in ["first", "second"].into_iter().enumerate() {
            let selected = source.join(name);
            fs::create_dir(&selected)?;
            fs::write(selected.join("saved.txt"), b"saved source")?;
            let registration = ProjectRegistration::new(
                selected.to_str().unwrap().into(),
                data.to_str().unwrap().into(),
                name.into(),
            )?;
            registry.register(&registration)?;
            let mut project = ProjectLease::acquire(&control, registration.binding)?;
            registry.admit_worker(&mut project)?;
            registry.record_watch_roots(&project, &[])?;
            let collection = Collection::initialize(
                &data,
                &registration.root,
                name,
                Incarnation::from_bytes([92 + index as u8; 16]),
            )?;
            let store = collection.selected_store()?;
            drop(store.lock()?);
            registrations.push(registration);
        }
        Ok(Self {
            _owner: owner,
            data,
            source,
            service,
            namespace,
            registrations,
        })
    }
    fn purge(&self) -> Result<()> {
        for registration in &self.registrations {
            let project = ProjectLease::acquire(self.service.root(), registration.binding)?;
            let mut complete = false;
            for _ in 0..8 {
                if Collection::purge_for_clean_break_turn(registration, &project)? {
                    complete = true;
                    break;
                }
            }
            ensure!(complete, "canonical collection purge did not finish");
        }
        Ok(())
    }
    fn finish(&self) -> Result<()> {
        for _ in 0..64 {
            if retire_clean_break_storage_turn(
                &self.data,
                &self.service,
                self.namespace,
                &self.registrations,
            )? {
                return Ok(());
            }
        }
        anyhow::bail!("bounded storage retirement did not finish")
    }
    fn preserved(&self) -> Result<()> {
        assert_eq!(
            fs::read(self.source.join("original.txt"))?,
            b"original source"
        );
        assert_eq!(
            fs::read(self.data.parent().unwrap().join("foreign-sibling.txt"))?,
            b"unrelated sibling"
        );
        for registration in &self.registrations {
            assert_eq!(
                fs::read(Path::new(&registration.root).join("saved.txt"))?,
                b"saved source"
            );
        }
        Ok(())
    }
}

#[test]
fn clean_break_storage_census_accepts_native_shared_root_writer_metadata_readonly() -> Result<()> {
    let fixture = Fixture::new()?;
    let before = fs::read(fixture.data.join(OWNER))?;
    validate_clean_break_storage(
        &fixture.data,
        fixture.service.root(),
        fixture.namespace,
        &fixture.registrations,
    )?;
    assert_eq!(fs::read(fixture.data.join(OWNER))?, before);
    assert!(
        fixture
            .data
            .join("collections-1.4/.writer-locks/first")
            .is_file()
    );
    assert!(fixture.data.join(".quota-1.4/ledger.lock").is_file());
    fixture.preserved()
}

#[test]
fn clean_break_storage_retires_exact_complete_native_root_and_keeps_sources() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.purge()?;
    fixture.finish()?;
    assert!(!fixture.data.exists());
    assert!(retire_clean_break_storage_turn(
        &fixture.data,
        &fixture.service,
        fixture.namespace,
        &fixture.registrations
    )?);
    fixture.preserved()
}

#[test]
fn clean_break_storage_refuses_subset_and_unknown_storage_before_deletion() -> Result<()> {
    let fixture = Fixture::new()?;
    let owner = fs::read(fixture.data.join(OWNER))?;
    assert!(
        validate_clean_break_storage(
            &fixture.data,
            fixture.service.root(),
            fixture.namespace,
            &fixture.registrations[..1]
        )
        .is_err()
    );
    fs::write(
        fixture.data.join("user-notes.txt"),
        b"unrecognized user data",
    )?;
    assert!(
        validate_clean_break_storage(
            &fixture.data,
            fixture.service.root(),
            fixture.namespace,
            &fixture.registrations
        )
        .is_err()
    );
    assert!(
        retire_clean_break_storage_turn(
            &fixture.data,
            &fixture.service,
            fixture.namespace,
            &fixture.registrations
        )
        .is_err()
    );
    assert_eq!(fs::read(fixture.data.join(OWNER))?, owner);
    assert_eq!(
        fs::read(fixture.data.join("user-notes.txt"))?,
        b"unrecognized user data"
    );
    fixture.preserved()
}

#[test]
fn clean_break_storage_live_root_credit_blocks_then_closed_credit_is_discarded() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.purge()?;
    let quota = Quota::open_service(&fixture.data)?;
    let reservation = quota.reserve_root(4096)?;
    let owner = fs::read(fixture.data.join(OWNER))?;
    assert!(
        retire_clean_break_storage_turn(
            &fixture.data,
            &fixture.service,
            fixture.namespace,
            &fixture.registrations
        )
        .is_err()
    );
    assert_eq!(fs::read(fixture.data.join(OWNER))?, owner);
    drop(reservation);
    fixture.finish()?;
    fixture.preserved()
}

#[test]
fn clean_break_storage_discards_known_pending_current_credit_without_replaying_old_work()
-> Result<()> {
    let fixture = Fixture::new()?;
    fixture.purge()?;
    // A real old writer can leave durable credit after losing its logical
    // payload. Whole-root reset has stronger authority than ordinary recovery.
    let logical = fixture.data.join("collections-1.4/first");
    let store = FileStore::new(&logical);
    let writer = store.lock()?;
    let quota = Quota::for_store(&store)?;
    let grant = quota.reserve_current(
        &writer,
        crate::quota::CurrentGrantScope {
            collection: "first".into(),
            incarnation: Incarnation::from_bytes([96; 16]),
            transaction: Sha256Digest::from_bytes([97; 32]),
        },
        4096,
    )?;
    drop(grant);
    drop(writer);
    fs::remove_dir(&logical)?;
    fixture.finish()?;
    fixture.preserved()
}

#[test]
fn clean_break_storage_live_writer_advisory_and_foreign_credit_are_preserved() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.purge()?;
    let advisory = OpenOptions::new()
        .read(true)
        .write(true)
        .open(fixture.data.join("collections-1.4/.writer-locks/first"))?;
    advisory.try_lock()?;
    let before = fs::read(fixture.data.join(OWNER))?;
    assert!(
        retire_clean_break_storage_turn(
            &fixture.data,
            &fixture.service,
            fixture.namespace,
            &fixture.registrations
        )
        .is_err()
    );
    assert_eq!(fs::read(fixture.data.join(OWNER))?, before);
    drop(advisory);
    let quota = Quota::open_service(&fixture.data)?;
    let reservation = quota.reserve("unknown-collection", 4096)?;
    assert!(
        retire_clean_break_storage_turn(
            &fixture.data,
            &fixture.service,
            fixture.namespace,
            &fixture.registrations
        )
        .is_err()
    );
    assert_eq!(fs::read(fixture.data.join(OWNER))?, before);
    drop(reservation);
    fixture.finish()?;
    fixture.preserved()
}

#[test]
fn clean_break_storage_exact_receipt_resumes_partial_disposal_and_refuses_new_files() -> Result<()>
{
    let fixture = Fixture::new()?;
    fixture.purge()?;
    assert!(
        retire_with_hook(
            &fixture.data,
            &fixture.service,
            fixture.namespace,
            &fixture.registrations,
            |effect| if effect {
                anyhow::bail!("simulated interruption after an exact unlink")
            } else {
                Ok(())
            }
        )
        .is_err()
    );
    assert!(
        fixture
            .service
            .root()
            .join(marker(&fixture.data, fixture.namespace)?)
            .is_file()
    );
    fs::write(fixture.data.join("new-user-file.txt"), b"new user bytes")?;
    assert!(
        retire_clean_break_storage_turn(
            &fixture.data,
            &fixture.service,
            fixture.namespace,
            &fixture.registrations
        )
        .is_err()
    );
    assert_eq!(
        fs::read(fixture.data.join("new-user-file.txt"))?,
        b"new user bytes"
    );
    // Remove only this test's newly-created sentinel, then resume the original
    // exact owned roster. No root, source or retained failure fixture is repaired.
    fs::remove_file(fixture.data.join("new-user-file.txt"))?;
    fixture.finish()?;
    fixture.preserved()
}

#[test]
fn clean_break_storage_receipt_rejects_replaced_native_root_object() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.purge()?;
    assert!(
        retire_with_hook(
            &fixture.data,
            &fixture.service,
            fixture.namespace,
            &fixture.registrations,
            |_| anyhow::bail!("simulated interruption after durable exact roster")
        )
        .is_err()
    );
    let displaced = fixture.data.parent().unwrap().join("original-data-object");
    fs::rename(&fixture.data, &displaced)?;
    fs::create_dir(&fixture.data)?;
    fs::write(fixture.data.join("user-data.txt"), b"replacement user data")?;
    assert!(
        retire_clean_break_storage_turn(
            &fixture.data,
            &fixture.service,
            fixture.namespace,
            &fixture.registrations
        )
        .is_err()
    );
    assert_eq!(
        fs::read(fixture.data.join("user-data.txt"))?,
        b"replacement user data"
    );
    assert!(displaced.join(OWNER).is_file());
    fixture.preserved()
}
