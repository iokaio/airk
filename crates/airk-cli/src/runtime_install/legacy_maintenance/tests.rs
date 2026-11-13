// SPDX-License-Identifier: Apache-2.0
//! Settlement of interrupted standalone adoption leftovers. The restart observer
//! replaces process launch; journals, fences, leases and selected payloads are real.
use super::*;

struct Fixture {
    owner: tempfile::TempDir,
    root: PathBuf,
    state: PathBuf,
    control: PathBuf,
    identity: String,
    _lock: File,
}

impl Fixture {
    fn new() -> Result<Self> {
        let (owner, root, identity) = super::super::tests::unselected_fixture()?;
        local_ipc::prepare_root(&root.join("control"))?;
        let lock = install_lock(&root)?;
        let (state, _) = crate::install_layout::retained_parameters(&root)?;
        let control = layout::control(&root)?;
        Ok(Self {
            owner,
            root,
            state,
            control,
            identity,
            _lock: lock,
        })
    }

    /// An earlier journal shape: extra fields stay readable for settlement.
    fn journal(&self, operation: Incarnation, checkpoint: Incarnation, phase: &str) -> Result<()> {
        replace_json(
            &self.root,
            JOURNAL,
            &serde_json::json!({"schema":1,"install":self.root,"control":self.control,
                "operation":operation,"checkpoint_operation":checkpoint,"phase":phase,
                "original_explicit_user_stop":false,"cycle_health":null,"cycle":0,
                "adoption_plans":[],"restoration":null,"restore_failure":null}),
        )
    }

    fn private_archives(&self, owner: serde_json::Value) -> Result<PathBuf> {
        let private = self.state.join(PRIVATE_ARCHIVES);
        fs::create_dir_all(private.join("archives/group/nested"))?;
        fs::create_dir_all(private.join("validation"))?;
        fs::write(private.join("archives/group/nested/index.bin"), b"copy")?;
        replace_json(&private, "owner.json", &owner)?;
        Ok(private)
    }

    fn owner_record(&self) -> serde_json::Value {
        serde_json::json!({"schema":1,"install":self.root,"state":self.state})
    }

    fn running_journal(&self, phase: &str, originally_stopped: bool) -> Result<()> {
        self.journal(operation(61), operation(62), phase)?;
        let mut journal: serde_json::Value =
            serde_json::from_slice(&fs::read(self.root.join(JOURNAL))?)?;
        journal["original_explicit_user_stop"] = serde_json::json!(originally_stopped);
        // Historical observations must never select the executable to restart.
        journal["cycle_health"] = serde_json::json!({"runtime":"f".repeat(64)});
        replace_json(&self.root, JOURNAL, &journal)
    }

    fn select(&self) -> Result<()> {
        replace_json(
            &self.root,
            "selected.json",
            &Selection {
                schema: 1,
                active: self.identity.clone(),
                previous: None,
            },
        )
    }
}

fn operation(byte: u8) -> Incarnation {
    Incarnation::from_bytes([byte; 16])
}

fn candidate() -> Result<airk_core::identity::Sha256Digest> {
    runtime_pair::current_executable_sha256()?.parse()
}

#[tokio::test]
async fn restoration_restarts_current_selection_after_owned_fence_and_leases_are_released()
-> Result<()> {
    let f = Fixture::new()?;
    f.running_journal("quiescent", false)?;
    f.select()?;
    crate::checkpoint_packet::arm_recovery(&f.control, operation(62), candidate()?)?;
    let mut calls = 0;
    let settled = settle_locked_with_start(&f.root, |root, approval, main| {
        calls += 1;
        assert_eq!(root, f.root);
        assert_eq!(approval.manifest_sha256, f.identity);
        assert_eq!(
            main,
            f.root.join("runtimes").join(&f.identity).join(main_name())
        );
        assert!(!crate::checkpoint_fence::admission_closed(&f.control).unwrap());
        // The observer is after every quiescence lease has been dropped.
        let exclusion = quiescent(&root).unwrap();
        drop(exclusion);
        std::future::ready(Ok(operation(63)))
    })
    .await?;
    assert_eq!(calls, 1);
    assert_eq!(
        settled.restored[1],
        serde_json::json!({"service":"restarted","runtime":f.identity,"instance":operation(63),
            "reason":"the adoption had stopped this running service"})
    );
    assert!(!f.root.join(JOURNAL).exists());
    require_settled(&f.root)?;
    assert!(
        settle_locked_with_start(
            &f.root,
            |_, _, _| -> std::future::Ready<Result<Incarnation>> {
                panic!("completed restoration must not start again")
            }
        )
        .await?
        .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn restoration_eligibility_preserves_user_stop_restored_and_foreign_fence_cases() -> Result<()>
{
    for (phase, originally_stopped, running, foreign_fence) in [
        ("restored", false, true, false),
        ("quiescent", true, true, false),
        ("quiescent", false, false, false),
        ("quiescent", false, true, true),
    ] {
        let f = Fixture::new()?;
        if running {
            f.running_journal(phase, originally_stopped)?;
        } else {
            f.journal(operation(61), operation(62), phase)?;
        }
        if foreign_fence {
            crate::checkpoint_packet::arm_recovery(&f.control, operation(65), candidate()?)?;
        }
        let settled = settle_locked_with_start(
            &f.root,
            |_, _, _| -> std::future::Ready<Result<Incarnation>> {
                panic!("an ineligible restoration must not launch a process")
            },
        )
        .await?;
        assert!(settled.restored.is_empty());
        assert!(!f.root.join(JOURNAL).exists());
        assert_eq!(
            crate::checkpoint_fence::fence_operation(&f.control)?,
            foreign_fence.then_some(operation(65))
        );
    }
    Ok(())
}

#[tokio::test]
async fn restoration_recovery_waits_for_native_owner_and_starter_refusal_is_reported_once()
-> Result<()> {
    let f = Fixture::new()?;
    f.running_journal("quiescent", false)?;
    f.select()?;
    crate::checkpoint_packet::arm_recovery(&f.control, operation(62), candidate()?)?;
    let owner = airk_store::service::ServiceLease::acquire(&f.control)?;
    let retained = settle_locked_with_start(
        &f.root,
        |_, _, _| -> std::future::Ready<Result<Incarnation>> {
            panic!("live native owner excludes restoration")
        },
    )
    .await?;
    assert_eq!(retained.left.len(), 1);
    assert!(f.root.join(JOURNAL).exists());
    assert!(require_settled(&f.root).is_err());
    drop(owner);
    let mut calls = 0;
    let recovered = settle_locked_with_start(&f.root, |_, _, _| {
        calls += 1;
        std::future::ready(Err(anyhow::anyhow!("observed starter refusal")))
    })
    .await?;
    assert_eq!(calls, 1);
    assert_eq!(
        recovered.left,
        vec![serde_json::json!({"service":"stopped","reason":
            "the adoption had stopped this service; restart was not possible (observed starter refusal)"})]
    );
    assert!(!f.root.join(JOURNAL).exists());
    assert!(!crate::checkpoint_fence::admission_closed(&f.control)?);
    require_settled(&f.root)?;
    assert!(settle_locked(&f.root).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn restoration_current_explicit_stop_refuses_before_the_start_observer() -> Result<()> {
    use airk_core::service_lifecycle::{LifecycleAction, LifecycleRequest};
    use airk_store::service::registry::ServiceRegistry;
    let f = Fixture::new()?;
    f.running_journal("quiescent", false)?;
    f.select()?;
    let mut registry = ServiceRegistry::open(&f.control, operation(66))?;
    let (namespace, epoch) = registry.request_epoch()?;
    registry.begin_lifecycle(&LifecycleRequest {
        caller: operation(67),
        key: airk_core::service::RequestKey {
            namespace,
            epoch,
            nonce: operation(68),
        },
        action: LifecycleAction::Stop,
        expected_instance: operation(69),
        runtime: f.identity.parse()?,
        manager: false,
    })?;
    drop(registry);
    let settled = settle_locked_with_start(
        &f.root,
        |_, _, _| -> std::future::Ready<Result<Incarnation>> {
            panic!("current explicit stop must be checked before process launch")
        },
    )
    .await?;
    assert_eq!(
        settled.left,
        vec![serde_json::json!({"service":"stopped","reason":
            "the adoption had stopped this service; restart was not possible (service_explicitly_stopped)"})]
    );
    assert!(ServiceRegistry::open_saved(&f.control)?.service_explicitly_stopped()?);
    assert!(!f.root.join(JOURNAL).exists());
    Ok(())
}

#[tokio::test]
async fn settlement_deletes_adoption_journals_and_owned_archives_once_and_keeps_everything_else()
-> Result<()> {
    let f = Fixture::new()?;
    let op = operation(31);
    f.journal(op, operation(32), "restored")?;
    for name in [
        format!("legacy-maintenance-state-{op}.json"),
        format!("legacy-maintenance-cycle-{op}-0.json"),
        format!("legacy-maintenance-restoration-{op}-0.json"),
    ] {
        fs::write(f.root.join(name), b"{}")?;
    }
    let registration =
        serde_json::json!({"root":"/source","data_root":"/adopted/data","collection":"saved"});
    replace_json(
        &f.root,
        &format!("{BATCH_PREFIX}{op}.json"),
        &serde_json::json!({"schema":2,"operation":op,"groups":[
            {"plan":{"setup":{"registration":registration,"key":{"nonce":"n"}}},"result":null},
            {"plan":{"setup":{"registration":{"root":"/ready"},"key":{"nonce":"r"}}},"result":{"status":"ready"}},
            {"plan":null,"result":null}
        ]}),
    )?;
    let private = f.private_archives(f.owner_record())?;
    // Everything below belongs to other owners and must stay byte-identical.
    let kept = [
        (
            f.root.join("legacy-cache-retirement.json"),
            b"cache".to_vec(),
        ),
        (
            f.control.join("legacy-retirement-1.4/receipt.json"),
            b"retirement".to_vec(),
        ),
        (
            f.owner.path().join("airk-adopted-indexes-0123/index.bin"),
            b"adopted".to_vec(),
        ),
        (
            f.owner.path().join("original/.airk/collections/x"),
            b"original".to_vec(),
        ),
    ];
    for (path, bytes) in &kept {
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(path, bytes)?;
    }

    let settled = settle_locked(&f.root).await?;
    assert!(!f.root.join(JOURNAL).exists());
    assert!(!private.exists());
    for entry in fs::read_dir(&f.root)? {
        let name = entry?.file_name().into_string().unwrap();
        assert!(
            !name.starts_with("legacy-maintenance") && !name.starts_with(BATCH_PREFIX),
            "{name} survived settlement"
        );
    }
    for (path, bytes) in &kept {
        assert_eq!(&fs::read(path)?, bytes, "{}", path.display());
    }
    assert_eq!(settled.deleted.len(), 6, "{:?}", settled.deleted);
    assert!(settled.restored.is_empty());
    // The group interrupted before Ready keeps its catalog rows and carries
    // rebuild guidance; a Ready group is reported as kept, without that code.
    assert_eq!(settled.left.len(), 2);
    assert!(settled.left[1]["code"].is_null());
    assert_eq!(settled.left[0]["registration"], registration);
    assert_eq!(
        settled.left[0]["code"],
        "interrupted_adoption_rebuild_required"
    );
    let guidance = settled.left[0]["reason"].as_str().unwrap();
    for expected in [
        "setting it up again from its original sources in Indexing",
        "retiring the inert entry if needed",
        "keep the machine and AIRK running (no shutdown or sleep) until that rebuild completes",
        "D8",
    ] {
        assert!(guidance.contains(expected), "{expected}: {guidance}");
    }
    require_settled(&f.root)?;

    let repeated = settle_locked(&f.root).await?;
    assert!(repeated.is_empty(), "settlement is idempotent");
    Ok(())
}

#[tokio::test]
async fn private_adoption_directory_without_exact_owner_or_with_links_is_kept() -> Result<()> {
    let f = Fixture::new()?;
    let mut foreign = f.owner_record();
    foreign["state"] = serde_json::json!(f.owner.path().join("another-state"));
    let private = f.private_archives(foreign)?;
    let settled = settle_locked(&f.root).await?;
    assert!(private.join("archives/group/nested/index.bin").exists());
    assert_eq!(settled.left.len(), 1, "{:?}", settled.left);
    assert!(settled.deleted.is_empty());

    fs::remove_file(private.join("owner.json"))?;
    let settled = settle_locked(&f.root).await?;
    assert!(private.join("archives/group/nested/index.bin").exists());
    assert!(
        settled.left[0]["reason"]
            .as_str()
            .unwrap()
            .contains("no owner record")
    );

    #[cfg(unix)]
    {
        replace_json(&private, "owner.json", &f.owner_record())?;
        let outside = f.owner.path().join("outside");
        fs::create_dir_all(&outside)?;
        fs::write(outside.join("user.bin"), b"user")?;
        std::os::unix::fs::symlink(&outside, private.join("archives/link"))?;
        settle_locked(&f.root).await?;
        assert!(private.join("owner.json").exists());
        assert_eq!(fs::read(outside.join("user.bin"))?, b"user");
    }
    Ok(())
}

#[tokio::test]
async fn foreign_fence_is_kept_and_own_fence_is_cleared_only_under_exclusion() -> Result<()> {
    let f = Fixture::new()?;
    let (op, own, other) = (operation(41), operation(42), operation(43));
    crate::checkpoint_packet::arm_recovery(&f.control, other, candidate()?)?;
    f.journal(op, own, "quiescent")?;
    let settled = settle_locked(&f.root).await?;
    assert_eq!(
        crate::checkpoint_fence::fence_operation(&f.control)?,
        Some(other),
        "a fence of another operation is never cleared"
    );
    assert!(!f.root.join(JOURNAL).exists());
    assert!(settled.restored.is_empty());
    assert_eq!(settled.left[0]["operation"], serde_json::json!(other));
    crate::checkpoint_packet::settle(&f.control, other)?;

    // The adoption's own fence while a native owner lease is still held: the
    // fence and journal are kept, and the manager-change gate refuses.
    crate::checkpoint_packet::arm_recovery(&f.control, own, candidate()?)?;
    f.journal(op, own, "quiescent")?;
    let owner = airk_store::service::ServiceLease::acquire(&f.control)?;
    let settled = settle_locked(&f.root).await?;
    assert!(f.root.join(JOURNAL).exists());
    assert_eq!(
        crate::checkpoint_fence::fence_operation(&f.control)?,
        Some(own)
    );
    assert_eq!(settled.left.len(), 1, "{:?}", settled.left);
    assert!(require_settled(&f.root).is_err());
    assert!(super::super::require_manager_change_settled(&f.root).is_err());

    drop(owner);
    let settled = settle_locked(&f.root).await?;
    assert!(!crate::checkpoint_fence::admission_closed(&f.control)?);
    assert!(!f.root.join(JOURNAL).exists());
    assert_eq!(settled.restored.len(), 1);
    require_settled(&f.root)?;
    super::super::require_manager_change_settled(&f.root)?;
    assert!(settle_locked(&f.root).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn unreadable_journal_is_removed_only_when_no_fence_remains() -> Result<()> {
    let f = Fixture::new()?;
    fs::write(f.root.join(JOURNAL), b"{unknown")?;
    crate::checkpoint_packet::arm_recovery(&f.control, operation(51), candidate()?)?;
    settle_locked(&f.root).await?;
    assert_eq!(fs::read(f.root.join(JOURNAL))?, b"{unknown");
    assert!(require_settled(&f.root).is_err());

    crate::checkpoint_packet::settle(&f.control, operation(51))?;
    let settled = settle_locked(&f.root).await?;
    assert!(!f.root.join(JOURNAL).exists());
    assert_eq!(settled.deleted.len(), 1);
    require_settled(&f.root)?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(f.root.join("missing-original"), f.root.join(JOURNAL))?;
        settle_locked(&f.root).await?;
        assert!(fs::symlink_metadata(f.root.join(JOURNAL)).is_ok());
        assert!(require_settled(&f.root).is_err());
    }
    Ok(())
}
