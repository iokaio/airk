// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::service::arbiter_assets::hash;
use airk_core::{arbiter::AssetRole, identity::Sha256Digest};
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Write, sync::Mutex};

fn fixture() -> Result<(tempfile::NamedTempFile, Asset)> {
    let bytes = vec![0x5a; 65543];
    let mut file = tempfile::NamedTempFile::new()?;
    file.write_all(&bytes)?;
    let asset = Asset {
        name: "actual-small-hash.bin".into(),
        role: AssetRole::Model,
        bytes: 65543,
        sha256: Sha256Digest::from_bytes(Sha256::digest(&bytes).into()),
    };
    Ok((file, asset))
}
fn observer(events: &Arc<Mutex<Vec<HashEvent>>>) -> Observer {
    let events = events.clone();
    Arc::new(move |event| {
        if let Ok(mut events) = events.try_lock() {
            events.push(event);
        }
    })
}
#[test]
fn real_file_hash_observes_positive_digest_progress_and_actual_return() -> Result<()> {
    let (file, asset) = fixture()?;
    let events = Arc::new(Mutex::new(Vec::new()));
    with_observer(observer(&events), || hash(&mut file.reopen()?, &asset))?;
    let events = events
        .lock()
        .map_err(|_| anyhow::anyhow!("observer poisoned"))?;
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].phase, HashPhase::Entered);
    assert_eq!(events[1].phase, HashPhase::Returned);
    assert!(events[0].bytes_hashed > 0 && events[0].bytes_hashed <= asset.bytes);
    assert_eq!(events[1].bytes_hashed, 65543);
    assert_eq!(events[0].asset, asset);
    assert_eq!(events[1].asset, asset);
    Ok(())
}
#[test]
fn observer_exit_retains_the_original_actual_digest_failure() -> Result<()> {
    let (file, mut asset) = fixture()?;
    asset.sha256 = Sha256Digest::from_bytes([0; 32]);
    let events = Arc::new(Mutex::new(Vec::new()));
    let error = with_observer(observer(&events), || hash(&mut file.reopen()?, &asset))
        .expect_err("bad digest must fail");
    assert_eq!(error.to_string(), "Arbiter asset bytes changed");
    let events = events
        .lock()
        .map_err(|_| anyhow::anyhow!("observer poisoned"))?;
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].phase, HashPhase::Returned);
    assert_eq!(events[1].bytes_hashed, 65543);
    Ok(())
}
#[test]
fn observer_panics_cannot_mask_original_io_failure_or_leak_thread_scope() -> Result<()> {
    let (file, mut asset) = fixture()?;
    asset.sha256 = Sha256Digest::from_bytes([0; 32]);
    let panicking: Observer = Arc::new(|_| panic!("private observer failure"));
    let error = with_observer(panicking, || hash(&mut file.reopen()?, &asset))
        .expect_err("bad digest must remain original error");
    assert_eq!(error.to_string(), "Arbiter asset bytes changed");
    let events = Arc::new(Mutex::new(Vec::new()));
    let _ = with_observer(observer(&events), || hash(&mut file.reopen()?, &asset));
    assert_eq!(
        events
            .lock()
            .map_err(|_| anyhow::anyhow!("observer poisoned"))?
            .len(),
        2
    );
    Ok(())
}
#[test]
fn nested_observers_restore_and_do_not_cross_threads_or_operation_return() -> Result<()> {
    let (file, asset) = fixture()?;
    let outer = Arc::new(Mutex::new(Vec::new()));
    let inner = Arc::new(Mutex::new(Vec::new()));
    with_observer(observer(&outer), || -> Result<()> {
        hash(&mut file.reopen()?, &asset)?;
        with_observer(observer(&inner), || hash(&mut file.reopen()?, &asset))?;
        let mut child_file: File = file.reopen()?;
        let child_asset = asset.clone();
        std::thread::spawn(move || hash(&mut child_file, &child_asset))
            .join()
            .map_err(|_| anyhow::anyhow!("child failed"))??;
        hash(&mut file.reopen()?, &asset)
    })?;
    hash(&mut file.reopen()?, &asset)?;
    assert_eq!(
        outer
            .lock()
            .map_err(|_| anyhow::anyhow!("observer poisoned"))?
            .len(),
        4
    );
    assert_eq!(
        inner
            .lock()
            .map_err(|_| anyhow::anyhow!("observer poisoned"))?
            .len(),
        2
    );
    Ok(())
}
#[test]
fn work_unwind_restores_the_original_thread_observer() -> Result<()> {
    let (file, asset) = fixture()?;
    let outer = Arc::new(Mutex::new(Vec::new()));
    let inner = Arc::new(Mutex::new(Vec::new()));
    with_observer(observer(&outer), || -> Result<()> {
        let unwind = std::panic::catch_unwind(AssertUnwindSafe(|| {
            with_observer(observer(&inner), || panic!("private work panic"))
        }));
        ensure!(unwind.is_err(), "work panic was swallowed");
        hash(&mut file.reopen()?, &asset)
    })?;
    assert_eq!(
        outer
            .lock()
            .map_err(|_| anyhow::anyhow!("observer poisoned"))?
            .len(),
        2
    );
    assert!(
        inner
            .lock()
            .map_err(|_| anyhow::anyhow!("observer poisoned"))?
            .is_empty()
    );
    Ok(())
}

#[test]
fn pre_read_length_refusal_cannot_invent_a_started_hash() -> Result<()> {
    let (file, mut asset) = fixture()?;
    asset.bytes += 1;
    let events = Arc::new(Mutex::new(Vec::new()));
    let error = with_observer(observer(&events), || hash(&mut file.reopen()?, &asset))
        .expect_err("wrong length must refuse before read");
    assert_eq!(error.to_string(), "Arbiter asset length differs");
    assert!(
        events
            .lock()
            .map_err(|_| anyhow::anyhow!("observer poisoned"))?
            .is_empty()
    );
    Ok(())
}
