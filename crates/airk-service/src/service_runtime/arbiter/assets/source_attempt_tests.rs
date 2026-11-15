// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::arbiter::{AssetDownload, AssetPart, Stage, Task};
use airk_store::service::{
    ServiceLease,
    arbiter_assets::{AssetWriter, Phase},
};
use sha2::{Digest, Sha256};
use std::io::Write;
type Calls = Arc<Mutex<Vec<usize>>>;

#[test]
fn source_failure_diagnostics_use_existing_bounded_content_free_records() -> Result<()> {
    use super::super::download::Failure;
    let asset = airk_core::identity::Sha256Digest::from_bytes([7; 32]);
    for failure in [
        Failure::Source(anyhow::anyhow!(
            "https://example.invalid/?signature=do_not_log"
        )),
        Failure::Local(anyhow::anyhow!("/private/source/path")),
        Failure::Canceled,
        Failure::Deadline,
    ] {
        let code = source_failure_code(asset, 2, &failure);
        assert!(code.len() <= 128);
        assert!(code.contains(&asset.to_string()));
        assert!(code.contains(failure.class()));
        assert!(!code.contains("https"));
        assert!(!code.contains("signature"));
        assert!(!code.contains("/private"));
        airk_core::service_logs::LogRecord {
            schema: 1,
            instance: airk_core::identity::Incarnation::from_bytes([1; 16]),
            observed_unix_ms: 1,
            kind: airk_core::service_logs::LogKind::ModelFailed,
            project: None,
            job: None,
            count: 1,
            code: Some(code),
        }
        .validate()?;
    }
    Ok(())
}

fn fixture(backups: usize) -> Result<(tempfile::TempDir, Arc<AssetStore>, Profile)> {
    let root = tempfile::tempdir()?;
    let store = AssetStore::open(
        Arc::new(ServiceLease::acquire(root.path())?),
        airk_core::identity::Sha256Digest::from_bytes([1; 32]),
        airk_core::identity::Incarnation::from_bytes([2; 16]),
    )?;
    let mut profile: Profile = serde_json::from_slice(include_bytes!(
        "../../../../../../fixtures/arbiter/profile-downloads.json"
    ))?;
    profile.qualification = None;
    profile.stage = Some(Stage::Context);
    profile.model = airk_core::arbiter::ModelChoice::ContextQwenCompact;
    profile.tasks = std::collections::BTreeSet::from([Task::ContextAction]);
    profile.task_policies = std::collections::BTreeMap::from([(
        Task::ContextAction,
        airk_retrieval::arbiter::context_action::policy_digest()?,
    )]);
    profile.abi = super::super::failure_tests::structural_context_abi();
    profile.assets.truncate(6);
    profile.peak_disk_bytes = 1 << 20;
    let full = airk_core::identity::Sha256Digest::from_bytes(Sha256::digest(b"helloworld").into());
    for asset in &mut profile.assets {
        asset.bytes = 10;
        asset.sha256 = full;
    }
    profile.compiler = full;
    profile.calibration = full;
    profile.downloads = profile.assets.iter().map(|asset| AssetDownload {
        asset_name: asset.name.clone(),
        parts: [b"hello".as_slice(), b"world".as_slice()].into_iter().enumerate().map(|(index, data)| AssetPart {
            url: format!("https://github.com/iokaio/airk/releases/download/v1.9.0/{}-part{:02}", asset.name, index + 1),
            backup_urls: (1..=backups).map(|ordinal| format!("https://github.com/iokaio/airk/releases/download/v1.9.0/{}-backup{}-part{:02}", asset.name, ordinal, index + 1)).collect(),
            bytes: 5,
            sha256: airk_core::identity::Sha256Digest::from_bytes(Sha256::digest(data).into()),
        }).collect(),
    }).collect();
    profile.validate_release_downloads()?;
    Ok((root, store, profile))
}

#[derive(Clone, Copy)]
enum Script {
    Success,
    PartialPrimary,
    AlwaysFail,
    LocalFailure,
    Cancel,
    Expire,
}
fn scripted(
    mode: Script,
    calls: Calls,
) -> impl for<'a> FnMut(
    &'a AssetPart,
    usize,
    &'a mut super::super::download::PartBudget,
    &'a mut AssetWriter,
    &'a CancellationToken,
) -> PartFuture<'a> {
    move |part, ordinal, budget, writer, cancel| {
        let calls = calls.clone();
        Box::pin(async move {
            calls.lock().unwrap().push(ordinal);
            budget.live()?;
            if matches!(mode, Script::Expire) {
                tokio::time::advance(Duration::from_secs(900)).await;
                return Err(super::super::download::Failure::Source(anyhow::anyhow!(
                    "late primary"
                )));
            }
            if matches!(mode, Script::LocalFailure) {
                return Err(super::super::download::Failure::Local(anyhow::anyhow!(
                    "local custody"
                )));
            }
            if matches!(mode, Script::Cancel) {
                cancel.cancel();
                return Err(super::super::download::Failure::Source(anyhow::anyhow!(
                    "source closed during cancel"
                )));
            }
            if matches!(mode, Script::AlwaysFail)
                || matches!(mode, Script::PartialPrimary)
                    && ordinal == 0
                    && part.url.ends_with("part02")
            {
                writer
                    .write_all(b"bad")
                    .map_err(|error| super::super::download::Failure::Local(error.into()))?;
                return Err(super::super::download::Failure::Source(anyhow::anyhow!(
                    "injected source disconnect"
                )));
            }
            let data = if part.url.ends_with("part01") {
                b"hello"
            } else {
                b"world"
            };
            super::super::download::consume(
                http_body_util::Full::new(bytes::Bytes::copy_from_slice(data)),
                part,
                writer,
            )
            .await
        })
    }
}

#[tokio::test]
async fn primary_success_skips_backup_and_partial_primary_restarts_whole_asset() -> Result<()> {
    for mode in [Script::Success, Script::PartialPrimary] {
        let (root, store, profile) = fixture(1)?;
        let transaction = store.begin(&profile, 1)?;
        let writer = transaction
            .asset(&profile.assets[0])?
            .context("missing writer")?;
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut request = scripted(mode, calls.clone());
        let writer = whole_asset(
            &transaction,
            &profile.downloads[0],
            writer,
            &CancellationToken::new(),
            &mut request,
            None,
        )
        .await?;
        transaction.publish(writer)?;
        assert_eq!(
            *calls.lock().unwrap(),
            if matches!(mode, Script::Success) {
                vec![0, 0]
            } else {
                vec![0, 0, 1, 1]
            }
        );
        assert_eq!(
            std::fs::read(
                root.path()
                    .join("arbiter-v3/assets")
                    .join(profile.manifest.to_string())
                    .join(&profile.assets[0].name)
            )?,
            b"helloworld"
        );
        assert!(store.pin(&profile).is_err());
    }
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn source_attempts_are_bounded_and_cancel_custody_or_original_deadline_never_fallback()
-> Result<()> {
    for mode in [
        Script::AlwaysFail,
        Script::LocalFailure,
        Script::Cancel,
        Script::Expire,
    ] {
        let (_root, store, profile) = fixture(2)?;
        let transaction = store.begin(&profile, 1)?;
        let writer = transaction
            .asset(&profile.assets[0])?
            .context("missing writer")?;
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut request = scripted(mode, calls.clone());
        assert!(
            whole_asset(
                &transaction,
                &profile.downloads[0],
                writer,
                &CancellationToken::new(),
                &mut request,
                None,
            )
            .await
            .is_err()
        );
        assert_eq!(
            *calls.lock().unwrap(),
            if matches!(mode, Script::AlwaysFail) {
                vec![0, 1, 2]
            } else {
                vec![0]
            }
        );
        drop(transaction);
        assert_eq!(store.phase(&profile)?, Some(Phase::Failed));
        assert!(store.pin(&profile).is_err());
    }
    Ok(())
}

#[tokio::test]
async fn absent_backup_and_precanceled_provisioning_never_invent_attempts() -> Result<()> {
    for canceled in [false, true] {
        let (_root, store, profile) = fixture(0)?;
        let transaction = store.begin(&profile, 1)?;
        let writer = transaction
            .asset(&profile.assets[0])?
            .context("missing writer")?;
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut request = scripted(Script::AlwaysFail, calls.clone());
        let cancel = CancellationToken::new();
        if canceled {
            cancel.cancel();
        }
        assert!(
            whole_asset(
                &transaction,
                &profile.downloads[0],
                writer,
                &cancel,
                &mut request,
                None,
            )
            .await
            .is_err()
        );
        assert_eq!(calls.lock().unwrap().len(), if canceled { 0 } else { 1 });
        drop(transaction);
        assert_ne!(store.phase(&profile)?, Some(Phase::Ready));
    }
    Ok(())
}
