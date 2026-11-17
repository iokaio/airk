// SPDX-License-Identifier: Apache-2.0
//! Read-only current4 device diagnostics; never an acquisition or migration path.
use super::*;
use airk_core::{
    arbiter::device::*,
    read_attempt::{ReadAttemptBudget, ReadClock},
};
fn live(attempt: ReadAttemptBudget) -> Result<()> {
    let clock = if cfg!(windows) {
        ReadClock::WindowsPerformanceCounter
    } else {
        ReadClock::UnixMonotonic
    };
    ensure!(
        !attempt
            .remaining_at(clock, airk_process::monotonic::now_ns()?)?
            .is_zero(),
        "original device observation deadline exhausted"
    );
    Ok(())
}
impl Owner {
    pub(in crate::service_runtime) fn device_read_status(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: airk_core::evidence::RevisionEnvelope,
        attempt: ReadAttemptBudget,
    ) -> Result<ScopedDeviceStatus> {
        live(attempt)?;
        self.context.validate_scope(worker, authority)?;
        self.focus.validate_scope(worker, authority)?;
        let scope = ScopeBinding {
            installation: self.context.installation,
            catalog: self.context.namespace,
            index_id: authority.index_id,
            authorization: authority.fingerprint,
            configuration_revision: 0,
            publication,
        };
        #[cfg(feature = "local-inference")]
        let value = self.read_current_device_snapshot(scope.clone());
        #[cfg(not(feature = "local-inference"))]
        let value = ScopedDeviceStatus::unavailable(
            scope.clone(),
            "Native optional inference is not present in this image.",
        );
        self.context.validate_scope(worker, authority)?;
        self.focus.validate_scope(worker, authority)?;
        live(attempt)?;
        value
    }
    #[cfg(feature = "local-inference")]
    fn read_current_device_snapshot(&self, scope: ScopeBinding) -> Result<ScopedDeviceStatus> {
        // load is a read-only strict decoder. NEVER call load_or_migrate or the
        // management current_device_configuration accessor on an evidence read.
        let store = self.configuration.device(device_management::binding()?);
        let document = match strict_current_configuration(&store) {
            Ok(Some(document)) => document,
            Ok(None) | Err(_) => {
                return ScopedDeviceStatus::unavailable(
                    scope,
                    "Current device configuration4 is absent, retained, or unavailable; no migration was performed.",
                );
            }
        };
        let snapshot = with_selection_snapshot(
            || {
                Ok((
                    self.focus.devices.read_diagnostic_stamp()?,
                    self.context.devices.read_diagnostic_stamp()?,
                ))
            },
            || -> Result<_> {
                let focus = self.focus.device_stage_status(&document)?;
                let context = self.context.device_stage_status(&document)?;
                let snapshot = DeviceStatus {
                    schema: "airk.arbiter.status/3".into(),
                    initialization: None,
                    configuration: document.clone(),
                    focus,
                    context,
                };
                snapshot.validate()?;
                ensure!(
                    strict_current_configuration(&store)? == Some(document.clone()),
                    "device snapshot changed"
                );
                ScopedDeviceStatus::from_status(snapshot, scope.clone())
            },
        );
        snapshot.or_else(|error| {
            if error.is::<airk_core::scoped_reads::ScopeReadRefused>() {
                return Err(error);
            }
            ScopedDeviceStatus::unavailable(
                scope,
                "Native device snapshot is unavailable or changed; no model operation was started.",
            )
        })
    }
}

#[cfg(feature = "local-inference")]
fn strict_current_configuration(
    store: &airk_store::service::arbiter::device::DeviceConfigurationStore<'_>,
) -> Result<Option<DeviceConfiguration>> {
    use airk_store::service::arbiter::device::StoredConfiguration;
    use sha2::{Digest, Sha256};
    // Management load intentionally synthesizes disabled current state on absence.
    // Evidence must first prove an actual canonical current4 file exists.
    let Some(before) = store.inspect_device_binding()? else {
        return Ok(None);
    };
    let StoredConfiguration::Device(value) = store.load()? else {
        return Ok(None);
    };
    let raw_sha = airk_core::identity::Sha256Digest::from_bytes(
        Sha256::digest(value.canonical_bytes()?).into(),
    );
    ensure!(
        before.binding == value.device_catalog
            && before.revision == value.revision
            && before.configuration_sha256 == raw_sha,
        "current4 changed after physical binding observation"
    );
    let after = store
        .inspect_device_binding()?
        .context("current4 disappeared during diagnostic read")?;
    ensure!(
        after.binding == before.binding
            && after.revision == before.revision
            && after.configuration_sha256 == before.configuration_sha256,
        "current4 binding changed during diagnostic read"
    );
    Ok(Some(value))
}
/// One bounded read-only selection/residency observation. Work progress need not
/// be atomic; a changed native selection or eligibility never delivers a pair.
#[cfg(feature = "local-inference")]
fn with_selection_snapshot<S: PartialEq, T>(
    sample: impl Fn() -> Result<S>,
    observe: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let before = sample()?;
    let value = observe()?;
    ensure!(
        sample()? == before,
        "native selection/residency changed during scoped device observation"
    );
    Ok(value)
}
#[cfg(all(test, feature = "local-inference"))]
mod tests;
