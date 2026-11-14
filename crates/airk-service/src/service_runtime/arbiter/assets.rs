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
impl Owner {
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
pub(super) async fn provision(
    profile: &Profile,
    store: &Arc<AssetStore>,
    revision: u64,
    cancel: &CancellationToken,
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
            for part in &download.parts {
                if let Err(error) = super::download::part(part, &mut writer, cancel).await {
                    if cancel.is_cancelled() {
                        transaction.cancel()?;
                    }
                    return Err(error);
                }
            }
            transaction.publish(writer)?;
        }
    }
    if cancel.is_cancelled() {
        transaction.cancel()?;
        anyhow::bail!("Arbiter provisioning canceled");
    }
    transaction.finish()
}
