// SPDX-License-Identifier: Apache-2.0
//! Execute only the exact accepted source mutation, retaining native owners
//! through the qualified job result and catalog completion.
use airk_core::{
    catalog_management::{CatalogOperation, catalog_operation_id},
    catalog_source_change::{SourceChangePlan, SourceConfigurationSnapshot},
    identity::{Incarnation, Sha256Digest},
    service_configuration::ConfigurationWork,
};
use airk_kernel::{Kernel, service_configuration::ConfigurationChanged};
use airk_store::{
    WriterLock,
    roots::RootConfig,
    service::{
        collection::{Collection, Selection},
        registry::{
            ServiceRegistry,
            catalog::source_change::{NativeSourceConfiguration, SourceChangeWork},
        },
    },
};
use anyhow::{Context, Result, ensure};
use std::path::Path;

pub(super) struct PreparedSourceConfiguration {
    begin: Incarnation,
    native: NativeSourceConfiguration,
    _selection: Selection,
    _writer: WriterLock,
}
impl PreparedSourceConfiguration {
    pub(super) fn native(&self) -> &NativeSourceConfiguration {
        &self.native
    }
    pub(super) fn complete(self, registry: &mut ServiceRegistry) -> Result<()> {
        registry.complete_catalog_source_change(self.begin, &self.native)?;
        Ok(())
    }
}

pub(super) fn prepare(
    kernel: &Kernel,
    accepted: &SourceChangeWork,
    configuration: &ConfigurationWork,
    engine: Sha256Digest,
) -> Result<PreparedSourceConfiguration> {
    let CatalogOperation::BeginIndexSourceChange { plan, .. } = &accepted.begin.operation else {
        anyhow::bail!("native source work must retain its accepted begin");
    };
    ensure!(
        accepted.job == plan.job
            && configuration.engine == engine
            && plan.engine_identity == engine
            && configuration.request == plan.configuration,
        ConfigurationChanged
    );
    let (selection, writer, native) = prepare_plan(kernel, plan, engine, true)?;
    Ok(PreparedSourceConfiguration {
        begin: catalog_operation_id(accepted.begin.caller, &accepted.begin.key)?,
        native,
        _selection: selection,
        _writer: writer,
    })
}

fn prepare_plan(
    kernel: &Kernel,
    plan: &SourceChangePlan,
    engine: Sha256Digest,
    apply: bool,
) -> Result<(Selection, WriterLock, NativeSourceConfiguration)> {
    plan.validate()?;
    ensure!(
        plan.engine_identity == engine
            && kernel.root().to_str() == Some(plan.registration.root.as_str())
            && kernel.data_dir().to_str() == Some(plan.registration.data_root.as_str())
            && kernel.collection() == plan.registration.collection,
        ConfigurationChanged
    );
    let collection = Collection::existing(
        Path::new(&plan.registration.data_root),
        &plan.registration.root,
        &plan.registration.collection,
    )?;
    let selection = collection.select()?;
    let writer = selection.store.lock()?;
    ensure!(
        selection.store.service_incarnation()? == Some(plan.configuration.incarnation),
        ConfigurationChanged
    );
    let mut config = RootConfig::load(selection.store.directory(), &plan.registration.root)?;
    let actual = SourceConfigurationSnapshot {
        primary_root: plan.registration.root.clone(),
        aliases: config.roots.clone(),
    };
    actual.validate()?;
    ensure!(
        actual == plan.before || actual == plan.after,
        ConfigurationChanged
    );
    ensure!(apply || actual == plan.after, ConfigurationChanged);
    // The removed source may be unavailable; only the desired roots must be
    // usable and retain their exact native identity before any mutation.
    airk_store::roots::validate_sources(
        Path::new(&plan.registration.root),
        Path::new(&plan.registration.data_root),
        &plan.after.aliases,
    )?;
    if actual == plan.before {
        ensure!(
            airk_store::service::collection::settings_digest_with_writer(
                &selection.store,
                &plan.registration.root,
                &writer
            )? == plan.configuration.settings,
            ConfigurationChanged
        );
        airk_kernel::service_execution::checkpoint()?;
        let quota = airk_store::quota::Quota::for_store(&selection.store)?;
        let reservation = quota.reserve(&plan.registration.collection, 96 * 1024)?;
        reservation.charge(96 * 1024)?;
        selection
            .store
            .invalidate_owned_state_before_mutation(&writer)?;
        config.roots = plan.after.aliases.clone();
        config.save(selection.store.directory())?;
    }
    let saved = RootConfig::load(selection.store.directory(), &plan.registration.root)?;
    let actual = SourceConfigurationSnapshot {
        primary_root: plan.registration.root.clone(),
        aliases: saved.roots,
    };
    ensure!(actual == plan.after, ConfigurationChanged);
    let native = NativeSourceConfiguration::from_validated_native_owner(
        plan.registration.clone(),
        engine,
        actual,
    )
    .context("native source configuration proof")?;
    Ok((selection, writer, native))
}

/// Once per owned worker startup, reconcile only already successful linked
/// jobs. A before/unknown RootConfig cannot be converted into success here.
pub(super) fn recover(
    kernel: &Kernel,
    registry: &mut ServiceRegistry,
    lease: &airk_store::service::ProjectLease,
    engine: Sha256Digest,
) -> Result<()> {
    for accepted in registry.pending_source_completions(lease)? {
        let CatalogOperation::BeginIndexSourceChange { plan, .. } = &accepted.begin.operation
        else {
            anyhow::bail!("pending source completion lacks accepted plan");
        };
        let (selection, writer, native) = match prepare_plan(kernel, plan, engine, false) {
            Ok(prepared) => prepared,
            Err(error) if error.is::<ConfigurationChanged>() => continue,
            Err(error) => return Err(error),
        };
        let prepared = PreparedSourceConfiguration {
            begin: catalog_operation_id(accepted.begin.caller, &accepted.begin.key)?,
            native,
            _selection: selection,
            _writer: writer,
        };
        prepared.complete(registry)?;
    }
    Ok(())
}

/// Explicit desired-only recovery. The current process retains its project
/// lease, selected incarnation and writer; this never applies RootConfig.
pub(super) fn resolve(
    kernel: &Kernel,
    registry: &mut ServiceRegistry,
    lease: &airk_store::service::ProjectLease,
    begin: Incarnation,
    engine: Sha256Digest,
) -> Result<airk_core::catalog_source_change::SourceChangeObservation> {
    let observed = registry.catalog_source_change_observation(begin)?;
    if observed.completion.is_some() {
        return Ok(observed);
    }
    let CatalogOperation::BeginIndexSourceChange { plan, .. } = &observed.begin.request.operation
    else {
        anyhow::bail!("source begin required");
    };
    ensure!(
        observed
            .job
            .as_ref()
            .is_some_and(|job| job.state == airk_core::service::JobState::Failed),
        "source resolution requires the saved failed job"
    );
    let (selection, writer, native) = prepare_plan(kernel, plan, engine, false)?;
    let proof=airk_store::service::registry::catalog::source_change::NativeDesiredSourceResolution::from_validated_native_owner(&native,&selection.store,&writer)?;
    registry.reconcile_failed_source_change(lease, begin, &proof)?;
    registry.catalog_source_change_observation(begin)
}

#[cfg(test)]
mod tests;
