// SPDX-License-Identifier: Apache-2.0
//! OS-adapter faults cannot turn a legacy approval into source/index deletion.
use super::*;
use std::cell::Cell;

struct Fake {
    spec: Spec,
    loaded: Cell<bool>,
    running: Cell<bool>,
    matches: Cell<bool>,
    available: Cell<bool>,
    disable_calls: Cell<u32>,
    fail_reload: Cell<bool>,
}
impl Backend for Fake {
    fn spec(&self) -> Result<Spec> {
        Ok(self.spec.clone())
    }
    fn observe(&self) -> Result<Observation> {
        Ok(Observation {
            owned: true,
            loaded: self.available.get().then_some(self.loaded.get()),
            running: self.available.get().then_some(self.running.get()),
            definition_matches: self.available.get().then_some(self.matches.get()),
            diagnostic: None,
        })
    }
    fn disable(&self) -> Result<()> {
        ensure!(
            self.matches.get() && self.available.get(),
            "foreign OS registration preserved"
        );
        self.disable_calls.set(self.disable_calls.get() + 1);
        self.running.set(false);
        Ok(())
    }
    fn state(&self) -> Result<(Option<bool>, Option<bool>)> {
        Ok((
            self.available.get().then_some(self.loaded.get()),
            self.available.get().then_some(self.running.get()),
        ))
    }
    fn reload(&self) -> Result<()> {
        ensure!(
            !self.fail_reload.replace(false),
            "injected manager reload failure"
        );
        if !exists(&self.spec.definition)? {
            self.loaded.set(false);
        }
        Ok(())
    }
}
fn fixture() -> Result<(tempfile::TempDir, PathBuf, Fake)> {
    let owner = tempfile::tempdir()?;
    let control = owner.path().join("control");
    fs::create_dir(&control)?;
    let source = owner.path().join("source");
    let data = owner.path().join("old-data");
    let metadata = owner.path().join("old-startup");
    for path in [&source, &data, &metadata] {
        fs::create_dir(path)?;
    }
    fs::write(source.join("original.rs"), b"original legacy source")?;
    fs::write(data.join("saved-settings.json"), b"saved project settings")?;
    fs::write(data.join("index.dat"), b"retained old index")?;
    let definition = metadata.join("unit.service");
    let receipt = metadata.join("receipt.json");
    fs::write(&definition, b"owned original startup definition")?;
    fs::write(&receipt, b"owned original startup receipt")?;
    let executable = owner
        .path()
        .join(if cfg!(windows) { "airk.exe" } else { "airk" });
    fs::write(&executable, b"old command must never execute")?;
    airk_store::service::registry::ServiceRegistry::open(
        &control,
        Incarnation::from_bytes([1; 16]),
    )?;
    let spec = Spec {
        id: "airk-fictional-startup".into(),
        target: Target {
            root: source,
            data_dir: data,
            collection: "legacy".into(),
            executable,
        },
        definition,
        receipt,
    };
    let backend = Fake {
        spec,
        loaded: Cell::new(true),
        running: Cell::new(true),
        matches: Cell::new(true),
        available: Cell::new(true),
        disable_calls: Cell::new(0),
        fail_reload: Cell::new(false),
    };
    Ok((owner, control, backend))
}
fn preserved(backend: &Fake) -> Result<()> {
    assert_eq!(
        fs::read(backend.spec.target.root.join("original.rs"))?,
        b"original legacy source"
    );
    assert_eq!(
        fs::read(backend.spec.target.data_dir.join("saved-settings.json"))?,
        b"saved project settings"
    );
    assert_eq!(
        fs::read(backend.spec.target.data_dir.join("index.dat"))?,
        b"retained old index"
    );
    assert_eq!(
        fs::read(&backend.spec.target.executable)?,
        b"old command must never execute"
    );
    Ok(())
}
#[test]
fn review_is_read_only_removal_is_scoped_and_completed_replay_never_changes_a_later_registration()
-> Result<()> {
    let (_owner, control, backend) = fixture()?;
    let before = fs::read(control.join("service-1.4.airk"))?;
    let review = inspect(&Journal::saved(&control)?, &backend)?;
    let approval = review.approval.context("approval missing")?;
    assert!(!exists(&control.join("legacy-retirement-1.4"))?);
    assert_eq!(fs::read(control.join("service-1.4.airk"))?, before);
    assert_eq!(backend.disable_calls.get(), 0);
    preserved(&backend)?;
    let result = remove(&control, &backend, approval)?;
    assert_eq!(result["startup_removed"], true);
    assert!(!exists(&backend.spec.definition)? && !exists(&backend.spec.receipt)?);
    assert_eq!(backend.disable_calls.get(), 1);
    assert_eq!(Journal::saved(&control)?.epoch(), review.preview.epoch + 1);
    fs::write(&backend.spec.definition, b"later unrelated startup")?;
    fs::write(&backend.spec.receipt, b"later unrelated receipt")?;
    backend.loaded.set(true);
    backend.running.set(true);
    backend.matches.set(false);
    assert_eq!(remove(&control, &backend, approval)?, result);
    assert_eq!(
        fs::read(&backend.spec.definition)?,
        b"later unrelated startup"
    );
    assert_eq!(fs::read(&backend.spec.receipt)?, b"later unrelated receipt");
    assert_eq!(backend.disable_calls.get(), 1);
    preserved(&backend)?;
    Journal::locked(&control)?.forget(approval)?;
    assert!(remove(&control, &backend, approval).is_err());
    assert_eq!(backend.disable_calls.get(), 1);
    preserved(&backend)
}
#[test]
fn foreign_or_unavailable_os_proof_never_stops_or_removes_legacy_files() -> Result<()> {
    for unavailable in [false, true] {
        let (_owner, control, backend) = fixture()?;
        backend.matches.set(false);
        backend.available.set(!unavailable);
        let review = inspect(&Journal::saved(&control)?, &backend)?;
        assert!(!review.preview.removal_available && review.approval.is_none());
        assert!(remove(&control, &backend, Sha256Digest::from_bytes([2; 32])).is_err());
        assert_eq!(backend.disable_calls.get(), 0);
        assert_eq!(
            fs::read(&backend.spec.definition)?,
            b"owned original startup definition"
        );
        assert_eq!(
            fs::read(&backend.spec.receipt)?,
            b"owned original startup receipt"
        );
        preserved(&backend)?;
    }
    Ok(())
}
#[test]
fn reload_failure_retains_exact_pending_approval_and_resumes_after_definition_deletion()
-> Result<()> {
    let (_owner, control, backend) = fixture()?;
    let review = inspect(&Journal::saved(&control)?, &backend)?;
    let approval = review.approval.unwrap();
    backend.fail_reload.set(true);
    assert!(remove(&control, &backend, approval).is_err());
    assert!(!exists(&backend.spec.definition)? && exists(&backend.spec.receipt)?);
    assert_eq!(
        Journal::saved(&control)?.get(approval)?.unwrap().phase,
        Phase::DefinitionRemoved
    );
    assert!(Journal::locked(&control)?.forget(approval).is_err());
    assert_eq!(backend.disable_calls.get(), 1);
    preserved(&backend)?;
    assert_eq!(
        remove(&control, &backend, approval)?["startup_removed"],
        true
    );
    assert_eq!(backend.disable_calls.get(), 1);
    preserved(&backend)
}
#[test]
fn crash_between_unlink_and_phase_receipt_resumes_without_repeating_manager_stop() -> Result<()> {
    for phase in [
        Phase::Disabled,
        Phase::DefinitionRemoved,
        Phase::ReceiptRemoved,
    ] {
        let (_owner, control, backend) = fixture()?;
        let review = inspect(&Journal::saved(&control)?, &backend)?;
        let approval = review.approval.unwrap();
        let mut journal = Journal::locked(&control)?;
        let mut intent = Intent {
            version: 1,
            review,
            phase: Phase::Planned,
        };
        journal.save(&intent)?;
        backend.disable()?;
        intent.phase = Phase::Disabled;
        journal.save(&intent)?;
        fs::remove_file(&backend.spec.definition)?;
        if phase >= Phase::DefinitionRemoved {
            backend.reload()?;
            intent.phase = Phase::DefinitionRemoved;
            journal.save(&intent)?;
            fs::remove_file(&backend.spec.receipt)?;
        }
        if phase == Phase::ReceiptRemoved {
            intent.phase = Phase::ReceiptRemoved;
            journal.save(&intent)?;
            journal.advance_epoch(intent.review.preview.epoch + 1)?;
        }
        drop(journal);
        assert_eq!(
            remove(&control, &backend, approval)?["startup_removed"],
            true
        );
        assert_eq!(backend.disable_calls.get(), 1);
        preserved(&backend)?;
    }
    Ok(())
}
#[test]
fn changed_receipt_and_unknown_running_worker_preserve_interrupted_removal() -> Result<()> {
    for change_receipt in [false, true] {
        let (_owner, control, backend) = fixture()?;
        let approval = inspect(&Journal::saved(&control)?, &backend)?
            .approval
            .unwrap();
        backend.fail_reload.set(true);
        assert!(remove(&control, &backend, approval).is_err());
        if change_receipt {
            fs::write(&backend.spec.receipt, b"unrelated user receipt")?;
        } else {
            backend.running.set(true);
        }
        assert!(remove(&control, &backend, approval).is_err());
        assert!(exists(&backend.spec.receipt)?);
        assert_eq!(backend.disable_calls.get(), 1);
        assert_ne!(
            Journal::saved(&control)?.get(approval)?.unwrap().phase,
            Phase::Complete
        );
        preserved(&backend)?;
    }
    Ok(())
}
#[test]
fn lost_initialized_owner_cannot_reset_request_epoch_and_unknown_initialization_files_survive()
-> Result<()> {
    let (_owner, control, backend) = fixture()?;
    let approval = inspect(&Journal::saved(&control)?, &backend)?
        .approval
        .unwrap();
    remove(&control, &backend, approval)?;
    fs::remove_file(control.join("legacy-retirement-1.4/owner.json"))?;
    assert!(Journal::saved(&control).is_err());
    assert!(Journal::locked(&control).is_err());
    assert!(!exists(&control.join("legacy-retirement-1.4/owner.json"))?);
    preserved(&backend)?;
    let (_owner, control, backend) = fixture()?;
    let directory = control.join("legacy-retirement-1.4");
    fs::create_dir(&directory)?;
    let unknown = directory.join(".airk-legacy-unknown");
    fs::write(&unknown, b"original user bytes")?;
    assert!(Journal::saved(&control).is_err());
    assert!(Journal::locked(&control).is_err());
    assert_eq!(fs::read(&unknown)?, b"original user bytes");
    preserved(&backend)
}
#[test]
fn stale_epochs_changed_metadata_and_file_aliases_never_authorize_removal() -> Result<()> {
    for change_file in [false, true] {
        let (_owner, control, backend) = fixture()?;
        let approval = inspect(&Journal::saved(&control)?, &backend)?
            .approval
            .unwrap();
        if change_file {
            fs::write(&backend.spec.definition, b"changed startup definition")?;
        } else {
            Journal::locked(&control)?.advance_epoch(1)?;
        }
        assert!(remove(&control, &backend, approval).is_err());
        assert_eq!(backend.disable_calls.get(), 0);
        assert!(exists(&backend.spec.definition)? && exists(&backend.spec.receipt)?);
        preserved(&backend)?;
    }
    let (_owner, control, backend) = fixture()?;
    fs::remove_file(&backend.spec.definition)?;
    fs::hard_link(
        backend.spec.target.root.join("original.rs"),
        &backend.spec.definition,
    )?;
    assert!(inspect(&Journal::saved(&control)?, &backend).is_err());
    preserved(&backend)
}

#[test]
fn corrupt_phase_receipts_do_not_report_success_or_repeat_an_os_effect() -> Result<()> {
    let (_owner, control, backend) = fixture()?;
    let review = inspect(&Journal::saved(&control)?, &backend)?;
    let approval = review.approval.unwrap();
    let mut journal = Journal::locked(&control)?;
    journal.save(&Intent {
        version: 1,
        review,
        phase: Phase::Planned,
    })?;
    drop(journal);
    let path = control
        .join("legacy-retirement-1.4")
        .join(format!("{approval}.json"));
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
    value["value"]["phase"] = serde_json::json!("complete");
    let corrupted = serde_json::to_vec(&value)?;
    fs::write(&path, &corrupted)?;
    assert!(remove(&control, &backend, approval).is_err());
    assert_eq!(backend.disable_calls.get(), 0);
    assert_eq!(fs::read(&path)?, corrupted);
    preserved(&backend)
}
