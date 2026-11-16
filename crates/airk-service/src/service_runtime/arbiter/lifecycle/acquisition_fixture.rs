// SPDX-License-Identifier: Apache-2.0
//! Actual private asset work; no public acquisition or release authority.
use super::*;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    sync::atomic::{AtomicBool, Ordering},
};
use tokio::sync::oneshot;

/// Captured only by an opaque EvaluationOwner retaining this actual lease/gate.
pub(in crate::service_runtime) struct AcquisitionFixture {
    store: Arc<AssetStore>,
    configuration: Arc<airk_store::service::arbiter::ConfigurationStore>,
    shared: Arc<Mutex<State>>,
    profile: Profile,
    expected: Configuration,
    sources: Option<BTreeMap<String, File>>,
}
#[derive(serde::Serialize)]
pub(in crate::service_runtime) struct Finished {
    pub elapsed_us: Option<u64>,
    pub original_cancellation_observed: bool,
    pub io_succeeded: bool,
    pub io_error: Option<String>,
    pub actual_phase: Option<Phase>,
    pub phase_error: Option<String>,
}
/// Observation failures never replace the actual Store result. Keep phase
/// verification diagnostic even when original Store I/O already failed.
fn report_result(
    io_result: Result<()>,
    phase_result: Result<Option<Phase>>,
    elapsed: Duration,
    canceled: bool,
    finished: oneshot::Sender<Finished>,
) -> Result<()> {
    let observation = Finished {
        elapsed_us: u64::try_from(elapsed.as_micros()).ok(),
        original_cancellation_observed: canceled,
        io_succeeded: io_result.is_ok(),
        io_error: io_result.as_ref().err().map(|error| format!("{error:#}")),
        actual_phase: phase_result.as_ref().ok().copied().flatten(),
        phase_error: phase_result
            .as_ref()
            .err()
            .map(|error| format!("{error:#}")),
    };
    // The receiver is retained by the real test across every control failure.
    // Even a dropped/misused observer cannot replace original I/O semantics.
    let _ = finished.send(observation);
    io_result?;
    let _ = phase_result?;
    Ok(())
}
impl AcquisitionFixture {
    pub(in crate::service_runtime::arbiter) fn capture(
        stage: &StageOwner,
        profile: Profile,
        sources: Option<BTreeMap<String, File>>,
    ) -> Result<Self> {
        let expected = stage.configuration()?;
        ensure!(
            stage.stage == Stage::Context && !expected.context.enabled,
            "actual acquisition fixture must keep Context disabled"
        );
        profile.validate_stage(Stage::Context)?;
        ensure!(
            profile.model == ModelChoice::ContextQwenLarge && profile.qualification.is_none(),
            "actual acquisition fixture requires current private Large"
        );
        ensure!(
            sources.is_some() || stage.assets.phase(&profile)? == Some(Phase::Ready),
            "actual cached verification requires the original native Ready receipt"
        );
        if sources.is_some() {
            ensure!(
                stage.assets.phase(&profile)?.is_none(),
                "actual finalization requires a fresh native asset transaction"
            );
        }
        Ok(Self {
            store: stage.assets.clone(),
            configuration: stage.configuration.clone(),
            shared: stage.lifecycle.clone(),
            profile,
            expected,
            sources,
        })
    }
    pub(in crate::service_runtime) async fn run(
        self,
        prepared: oneshot::Sender<CancellationToken>,
        release: oneshot::Receiver<()>,
        entered: oneshot::Sender<airk_store::service::arbiter_assets::hash_observer::HashEvent>,
        finished: oneshot::Sender<Finished>,
        active: Arc<AtomicBool>,
    ) -> Result<()> {
        let (serial, cancel) = start(&self.shared, self.profile.manifest, None)?;
        let shared = self.shared.clone();
        let result = super::super::assets::owned_asset_operation(async move {
            current(&self.configuration, &self.expected, Stage::Context, &cancel)?;
            let transaction = if let Some(mut sources) = self.sources {
                phase(
                    &self.shared,
                    serial,
                    Availability::Verifying,
                    "Private actual asset publication; not release qualified.",
                )?;
                let transaction = self.store.begin(&self.profile, self.expected.revision)?;
                let mut buffer = [0_u8; 65536];
                for asset in &self.profile.assets {
                    current(&self.configuration, &self.expected, Stage::Context, &cancel)?;
                    let mut writer = transaction
                        .asset(asset)?
                        .context("fresh acquisition fixture reused a published object")?;
                    let mut input = sources
                        .remove(&asset.name)
                        .context("private source descriptor missing")?;
                    ensure!(
                        input.metadata()?.is_file() && input.metadata()?.len() == asset.bytes,
                        "private source length differs"
                    );
                    input.seek(SeekFrom::Start(0))?;
                    let mut remaining = asset.bytes;
                    while remaining > 0 {
                        current(&self.configuration, &self.expected, Stage::Context, &cancel)?;
                        let limit = usize::try_from(remaining.min(buffer.len() as u64))?;
                        let read = input.read(&mut buffer[..limit])?;
                        ensure!(read > 0, "private source shortened");
                        writer.write_all(&buffer[..read])?;
                        remaining -= u64::try_from(read)?;
                    }
                    let mut extra = [0_u8; 1];
                    ensure!(
                        input.read(&mut extra)? == 0 && input.metadata()?.len() == asset.bytes,
                        "private source changed"
                    );
                    transaction.publish(writer)?;
                }
                ensure!(
                    sources.is_empty(),
                    "private source roster has extra objects"
                );
                Some(transaction)
            } else {
                None
            };
            prepared
                .send(cancel.clone())
                .map_err(|_| anyhow::anyhow!("actual acquisition preparation observer closed"))?;
            release
                .await
                .context("actual acquisition entry gate closed")?;
            current(&self.configuration, &self.expected, Stage::Context, &cancel)?;
            use airk_store::service::arbiter_assets::hash_observer::{
                HashPhase, Observer, with_observer,
            };
            let target = self
                .profile
                .assets
                .iter()
                .find(|asset| asset.name == "model.onnx.data" && asset.bytes >= 16_000_000_000)
                .context("actual Large hash target missing")?
                .clone();
            let entry = Arc::new(Mutex::new(Some(entered)));
            let observer: Observer = Arc::new(move |event| {
                if event.asset != target {
                    return;
                }
                match event.phase {
                    HashPhase::Entered => {
                        // Store emitted this AFTER positive read and digest
                        // progress. Only atomics/nonblocking channel delivery
                        // occur here; no delay, gate, source or hash mutation.
                        if event.bytes_hashed > 0 {
                            active.store(true, Ordering::Release);
                            if let Ok(mut sender) = entry.try_lock()
                                && let Some(sender) = sender.take()
                            {
                                let _ = sender.send(event);
                            }
                        }
                    }
                    HashPhase::Returned => active.store(false, Ordering::Release),
                }
            });
            let began = Instant::now();
            let io_result = with_observer(observer, || match transaction {
                Some(transaction) => transaction.finish(),
                None => self.store.pin(&self.profile).map(drop),
            });
            let elapsed = began.elapsed();
            let phase_result = self.store.phase(&self.profile);
            report_result(
                io_result,
                phase_result,
                elapsed,
                cancel.is_cancelled(),
                finished,
            )?;
            // Storage can become Ready after a cancellation during its synchronous
            // hash. That authentic storage result never authorizes activation.
            current(&self.configuration, &self.expected, Stage::Context, &cancel)
        })
        .await;
        complete(&shared, serial, &result);
        result
    }
}

mod tests;
