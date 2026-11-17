// SPDX-License-Identifier: Apache-2.0
//! Runtime asset adapter retains descriptor pins and the actual service lease.
use super::*;
use airk_retrieval::arbiter::runtime::AssetSource;
use airk_store::service::arbiter_assets::{AssetStore, PinnedAssets};
use tokio_util::sync::CancellationToken;
struct Source {
    store: Arc<AssetStore>,
    profile: Profile,
    pinned: Mutex<Option<PinnedAssets>>,
}
impl AssetSource for Source {
    fn read_asset(&self, asset: &Asset) -> Result<Vec<u8>> {
        let mut pins = self
            .pinned
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter asset source unavailable"))?;
        if pins.is_none() {
            *pins = Some(self.store.pin(&self.profile)?);
        }
        pins.as_ref()
            .context("Arbiter asset source not verified")?
            .read(asset)
    }
}
impl StageOwner {
    pub(super) fn execution_profile(
        &self,
        manifest: airk_core::identity::Sha256Digest,
    ) -> Result<airk_retrieval::arbiter::runtime::ExecutionProfile> {
        #[cfg(feature = "test-support")]
        if let Some(catalog) = &self.evaluation {
            return catalog.execution_profile(manifest);
        }
        airk_retrieval::arbiter::runtime::ExecutionProfile::from_release(manifest)
    }
    pub(super) fn profile_descriptions(&self) -> Result<Vec<Profile>> {
        #[cfg(feature = "test-support")]
        if let Some(catalog) = &self.evaluation {
            return Ok(catalog.profiles());
        }
        airk_retrieval::arbiter::runtime::catalog_profiles()
    }
    pub(super) fn asset_source(
        &self,
        manifest: airk_core::identity::Sha256Digest,
    ) -> Result<Box<dyn AssetSource + Send>> {
        let profile = self.execution_profile(manifest)?;
        Ok(Box::new(Source {
            store: self.assets.clone(),
            profile: profile.description().clone(),
            pinned: Mutex::new(None),
        }))
    }
}
/// One joined owner runs explicit asset I/O and whole-file verification away
/// from the service control executor. It retains the actual Store/ServiceLease
/// and original cancellation/deadline state through real completion; no cancel
/// select drops the join or reports that unfinished native work has settled.
pub(super) async fn owned_asset_operation<T>(
    work: impl std::future::Future<Output = Result<T>> + Send + 'static,
) -> Result<T>
where
    T: Send + 'static,
{
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || runtime.block_on(work))
        .await
        .context("Arbiter owned asset operation closed")?
}

pub(super) async fn provision(
    profile: &Profile,
    store: &Arc<AssetStore>,
    revision: u64,
    cancel: &CancellationToken,
    source_log: Option<&airk_host::service_logs::Log>,
) -> Result<()> {
    let profile = profile.clone();
    let store = store.clone();
    let cancel = cancel.clone();
    let source_log = source_log.cloned();
    owned_asset_operation(async move {
        provision_owned(&profile, &store, revision, &cancel, source_log.as_ref()).await
    })
    .await
}

async fn provision_owned(
    profile: &Profile,
    store: &Arc<AssetStore>,
    revision: u64,
    cancel: &CancellationToken,
    source_log: Option<&airk_host::service_logs::Log>,
) -> Result<()> {
    profile.validate_release_downloads()?;
    let probe = store.clone();
    let declared = profile.clone();
    if tokio::task::spawn_blocking(move || probe.pin(&declared))
        .await
        .context("Arbiter existing-asset verification closed")?
        .is_ok()
    {
        return Ok(());
    }
    let transaction = store.begin(profile, revision)?;
    for asset in &profile.assets {
        if cancel.is_cancelled() {
            transaction.cancel()?;
            anyhow::bail!("Arbiter provisioning canceled");
        }
        if let Some(mut writer) = transaction.asset(asset)? {
            let download = profile
                .downloads
                .iter()
                .find(|download| download.asset_name == asset.name)
                .context("Arbiter asset transport missing")?;
            writer = match whole_asset(
                &transaction,
                download,
                writer,
                cancel,
                &mut native_part,
                source_log.map(|log| (log, asset.sha256)),
            )
            .await
            {
                Ok(writer) => writer,
                Err(error) => {
                    if cancel.is_cancelled() {
                        transaction.cancel()?;
                    }
                    return Err(error);
                }
            };
            transaction.publish(writer)?;
        }
    }
    if cancel.is_cancelled() {
        transaction.cancel()?;
        anyhow::bail!("Arbiter provisioning canceled");
    }
    transaction.finish()
}

type PartFuture<'a> = std::pin::Pin<
    Box<
        dyn std::future::Future<Output = std::result::Result<(), super::download::Failure>>
            + Send
            + 'a,
    >,
>;
fn native_part<'a>(
    part: &'a AssetPart,
    ordinal: usize,
    budget: &'a mut super::download::PartBudget,
    writer: &'a mut airk_store::service::arbiter_assets::AssetWriter,
    cancel: &'a CancellationToken,
) -> PartFuture<'a> {
    Box::pin(super::download::part(part, ordinal, budget, writer, cancel))
}
async fn whole_asset<F>(
    transaction: &airk_store::service::arbiter_assets::Transaction,
    download: &airk_core::arbiter::AssetDownload,
    mut writer: airk_store::service::arbiter_assets::AssetWriter,
    cancel: &CancellationToken,
    request: &mut F,
    diagnostics: Option<(
        &airk_host::service_logs::Log,
        airk_core::identity::Sha256Digest,
    )>,
) -> Result<airk_store::service::arbiter_assets::AssetWriter>
where
    F: for<'a> FnMut(
        &'a AssetPart,
        usize,
        &'a mut super::download::PartBudget,
        &'a mut airk_store::service::arbiter_assets::AssetWriter,
        &'a CancellationToken,
    ) -> PartFuture<'a>,
{
    let sources = download.source_count()?;
    let mut budgets: Vec<Option<super::download::PartBudget>> =
        (0..download.parts.len()).map(|_| None).collect();
    let mut failures = Vec::new();
    for ordinal in 0..sources {
        ensure!(!cancel.is_cancelled(), "Arbiter provisioning canceled");
        for budget in budgets.iter().flatten() {
            budget.live()?;
        }
        let mut failed = None;
        for (index, part) in download.parts.iter().enumerate() {
            let budget = budgets[index]
                .get_or_insert_with(|| super::download::PartBudget::for_part(part.bytes));
            if let Err(error) = request(part, ordinal, budget, &mut writer, cancel).await {
                failed = Some(error);
                break;
            }
        }
        match failed {
            None => return Ok(writer),
            Some(error) => {
                failures.push(format!("source ordinal {ordinal}: {}", error.class()));
                if let Some((log, asset_sha256)) = diagnostics {
                    log.record_code(
                        airk_core::service_logs::LogKind::ModelFailed,
                        None,
                        None,
                        Some(&source_failure_code(asset_sha256, ordinal, &error)),
                    );
                }
                if !error.can_fallback() || ordinal + 1 == sources || cancel.is_cancelled() {
                    return Err(anyhow::anyhow!(
                        "Arbiter asset source attempts stopped: {}",
                        failures.join("; ")
                    ))
                    .context(error);
                }
                for budget in budgets.iter().flatten() {
                    budget.live()?;
                }
                writer = transaction.restart_failed_asset_if(writer, || {
                    ensure!(!cancel.is_cancelled(), "Arbiter provisioning canceled");
                    for budget in budgets.iter().flatten() {
                        budget.live()?;
                    }
                    Ok(())
                })?;
            }
        }
    }
    anyhow::bail!("Arbiter complete source roster exhausted")
}

#[cfg(test)]
mod operation_tests;
#[cfg(test)]
mod source_attempt_tests;

fn source_failure_code(
    asset: airk_core::identity::Sha256Digest,
    ordinal: usize,
    failure: &super::download::Failure,
) -> String {
    format!("arbiter_source_{asset}_{ordinal}_{}", failure.class())
}

/// Source custody only, never an execution capability. Runtime verifies the
/// actual independently admitted ExecutionProfile before consuming these pins.
pub(super) fn source_for_store(
    profile: Profile,
    store: Arc<AssetStore>,
) -> Box<dyn AssetSource + Send> {
    Box::new(Source {
        store,
        profile,
        pinned: Mutex::new(None),
    })
}
