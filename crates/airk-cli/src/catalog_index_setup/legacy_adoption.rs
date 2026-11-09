// SPDX-License-Identifier: Apache-2.0
//! Native saved-only source review. Old source strings are evidence labels,
//! never fabricated live-directory ownership or permission to rescan them.
use super::*;
use airk_core::{catalog_legacy_adoption::LegacyAdoptionPlan, source_observations::SourceRelation};
use airk_store::{
    legacy_adoption::VerifiedLegacyArchive, service::registry::catalog::setup::SavedAdoptionSources,
};

fn same_saved_path(left: &Path, right: &Path) -> Result<bool> {
    Ok(airk_store::service::registry::source_relation(left, right)? == Some(SourceRelation::Same))
}
pub(crate) fn validate_origin(
    plan: &LegacyAdoptionPlan,
    origin: &mut crate::runtime_legacy_origin::NativeLegacyOrigin,
) -> Result<()> {
    plan.validate()?;
    origin.revalidate()?;
    ensure!(
        origin.identity()? == plan.origin_sha256,
        "saved adoption original configuration identity changed"
    );
    let observed = origin.observation();
    ensure!(
        observed.config_sha256.parse::<Sha256Digest>()? == plan.config_sha256
            && observed.executable_sha256.parse::<Sha256Digest>()? == plan.executable_sha256,
        "saved adoption original configuration/image differs from captured bytes"
    );
    validate_scope(plan, origin)
}

/// Additional declarations keep their own identities and select the same
/// native original backing and archived source scope as the primary claim.
pub(crate) fn validate_scope(
    plan: &LegacyAdoptionPlan,
    origin: &mut crate::runtime_legacy_origin::NativeLegacyOrigin,
) -> Result<()> {
    plan.validate()?;
    origin.revalidate()?;
    let observed = origin.observation();
    let declaration = &observed.declaration;
    let scope = origin.standalone_scope()?;
    ensure!(
        declaration.role == "mcp"
            && !declaration.no_default_project
            && declaration.service_install_dir.is_none()
            && declaration.service_runtime_dir.is_none()
            && declaration.service_project.is_none(),
        "standalone adoption requires its exact original explicit project declaration"
    );
    ensure!(
        same_saved_path(&scope.root, Path::new(&plan.original_registration.root))?
            && same_saved_path(
                &scope.data_dir,
                Path::new(&plan.original_registration.data_root)
            )?
            && scope.collection == plan.original_registration.collection,
        "original declaration does not select the exact adopted source/storage/collection"
    );
    for additional in &declaration.additional_roots {
        let saved = plan
            .setup
            .sources
            .iter()
            .find(|source| source.aliases.contains(&additional.alias))
            .map(|source| &source.canonical_path)
            .context("original additional root is absent from saved evidence")?;
        ensure!(
            same_saved_path(&additional.path, Path::new(saved))?,
            "original additional root differs from saved evidence"
        );
    }
    origin.revalidate()?;
    Ok(())
}
pub(crate) fn review(
    control: &Path,
    plan: &LegacyAdoptionPlan,
    archive: &VerifiedLegacyArchive,
    origin: &mut crate::runtime_legacy_origin::NativeLegacyOrigin,
) -> Result<NativeSetupReview> {
    validate_origin(plan, origin)?;
    let saved = SavedAdoptionSources::from_verified_archive(plan, archive)?;
    crate::runtime_install::validate_control_root(control)?;
    let control = control.canonicalize()?;
    let request = &plan.setup;
    let registry = ServiceRegistry::open_saved(&control)?;
    let state = registry
        .catalog_state()?
        .context("native adoption catalog is absent")?;
    ensure!(
        state.activated && state.namespace == request.key.namespace,
        "native adoption catalog changed"
    );
    let previous = registry.index_setup_receipt(request.caller, &request.key)?;
    if let Some(previous) = &previous {
        ensure!(
            previous.request == *request
                && previous.legacy_adoption == Some(plan.identity()?)
                && previous.stage != SetupStage::Ready,
            "saved adoption reservation changed or is already ready"
        );
    } else {
        ensure!(
            state.revision == request.expected_catalog_revision,
            "saved adoption review is stale"
        );
    }
    let data = Path::new(&request.registration.data_root);
    ensure!(
        airk_store::service::data_scope::canonical_location(data)? == data,
        "saved adoption destination must be canonical native storage"
    );
    ensure!(
        airk_store::service::registry::source_relation(data, &control)?.is_none(),
        "saved adoption destination overlaps native control"
    );
    for source in &request.sources {
        let source = Path::new(&source.canonical_path);
        ensure!(
            airk_store::service::registry::source_relation(source, data)?.is_none()
                && airk_store::service::registry::source_relation(source, &control)?.is_none(),
            "saved adoption source overlaps private native storage"
        );
    }
    let parent = DirectoryPin::open(
        data.parent()
            .context("saved adoption destination has no parent")?,
    )?;
    let storage_action =
        validate_destination(data, &control, request.key.namespace, previous.is_some())?;
    let existing_data = if storage_action == SetupStorageAction::CreateMissingLeaf {
        None
    } else {
        Some(DirectoryPin::open(data)?)
    };
    let metadata_plan =
        airk_store::service::registry::catalog::setup::validate_saved_adoption_metadata_plan(
            &control, &saved,
        )?;
    origin.revalidate()?;
    Ok(NativeSetupReview {
        control,
        request: request.clone(),
        parent,
        existing_data,
        sources: vec![],
        storage_action,
        metadata_plan,
        legacy_adoption: Some(plan.identity()?),
        saved_adoption: Some(saved),
    })
}
#[cfg(test)]
mod tests;
