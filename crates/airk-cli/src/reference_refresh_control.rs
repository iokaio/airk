// SPDX-License-Identifier: Apache-2.0
//! Native-only refresh admission and existing Configure-job completion.
use airk_core::{
    catalog_management::{CatalogEntity, CatalogItem},
    catalog_refresh::{RefreshMode, RefreshSelection},
    identity::Incarnation,
    service::{JobOutcome, JobRecord, ProjectBinding, RequestKey, ScopedConnectRequest},
    service_configuration::{ConfigurationRequest, ProjectConfiguration},
};
use airk_store::service::{ProjectLease, registry::ServiceRegistry};
use anyhow::{Context, Result, ensure};

fn selected(request: &ConfigurationRequest) -> Result<&RefreshSelection> {
    request.validate()?;
    ensure!(!request.prepare, "refresh never defers native preparation");
    let ProjectConfiguration::RefreshReferences { selection } = &request.configuration else {
        anyhow::bail!("native refresh selection missing");
    };
    ensure!(
        selection.members.len() == 1,
        "native refresh currently admits one exact selected member"
    );
    Ok(selection)
}

/// Reuse canonical authenticated host/grant/catalog/backing construction.
/// This creates no grant, default preference, bearer credential or worker.
fn qualify(
    registry: &mut ServiceRegistry,
    project: ProjectBinding,
    request: &ConfigurationRequest,
) -> Result<airk_kernel::Kernel> {
    let selection = selected(request)?;
    let member = &selection.members[0];
    let scoped = ScopedConnectRequest {
        schema: 1,
        context_key: selection.selection.context_key,
        namespace: selection.selection.namespace,
        expected_catalog_revision: selection.selection.expected_catalog_revision,
        grant_id: selection.selection.grant_id,
        expected_grant_revision: selection.selection.expected_grant_revision,
        index_id: member.index_id,
        project_ids: selection.selection.project_ids.clone(),
    };
    let native = crate::catalog_coordinator::native_scoped_selection(registry, &scoped)?;
    ensure!(
        native.backing == project,
        "refresh target differs from selected native backing"
    );
    let authority = crate::catalog_authority::build_catalog_read_authority(registry, &native)?;
    let detail = registry.catalog_detail(
        &CatalogEntity::Index {
            id: member.index_id,
        },
        Some(scoped.expected_catalog_revision),
    )?;
    let Some(CatalogItem::Index(index)) = detail.item else {
        anyhow::bail!("refresh index unavailable")
    };
    ensure!(index.backing.binding == project, "refresh backing changed");
    // Match the ordinary native worker's read-only model context. This binds
    // immutable descriptors/runtime only: no provisioning, model weights,
    // inference, source scan or Current mutation is performed by qualification.
    let kernel = crate::bundled_assets::bind_service_models(
        airk_kernel::Kernel::open_existing_service_observer(&index.backing)?
            .with_expected_source_policy(registry.catalog_source_policy(index.id)?)?,
        registry.control_root(),
    )?;
    let scoped_kernel = kernel.with_scoped_reads(authority.clone())?;
    // First-wave selected refresh AND full fallback require full-index metadata
    // authority. Partial Project/path grants are unsupported; never broaden them.
    // Root/alias/profile metadata follows authenticated complete coverage.
    scoped_kernel.qualify_reference_refresh(selection, &authority.sources)?;
    kernel.validate_service_configuration(project, request)?;
    let observed = kernel.inspect_service(project)?;
    ensure!(
        observed.source_state == airk_core::service_inspection::SourceState::Available
            && observed.recovery_required == Some(false),
        "refresh sources/recovery are not ready; preserve publication and review again"
    );
    Ok(kernel)
}

pub(crate) fn admit(
    registry: &mut ServiceRegistry,
    project: ProjectBinding,
    caller: Incarnation,
    key: &RequestKey,
    request: &ConfigurationRequest,
) -> Result<JobRecord> {
    let selection = selected(request)?;
    ensure!(
        selection.caller == caller && selection.key == *key,
        "refresh exact caller/key binding changed"
    );
    // Exact already-accepted work remains a dated receipt after later catalog
    // changes. Store compares the complete immutable request before returning.
    if registry.request_receipt(caller, key)?.0.is_none() {
        qualify(registry, project, request)?;
    }
    registry.enqueue_native_reference_refresh(
        project,
        caller,
        key,
        request,
        crate::runtime_pair::engine_identity()?,
    )
}

/// Only a fresh pre-admission typed refusal can claim no accepted job.
/// An exact already-accepted key always returns its historical accepted job.
pub(crate) fn admit_response(
    registry: &mut ServiceRegistry,
    project: ProjectBinding,
    caller: Incarnation,
    key: &RequestKey,
    request: &ConfigurationRequest,
) -> Result<airk_core::service::ControlResponse> {
    match admit(registry, project, caller, key, request) {
        Ok(job) => Ok(airk_core::service::ControlResponse::Job { job }),
        Err(error) => {
            if let Some(review) =
                error.downcast_ref::<airk_core::catalog_refresh::RefreshReviewRequired>()
            {
                review.validate()?;
                Ok(
                    airk_core::service::ControlResponse::ReferenceRefreshReviewRequired {
                        project,
                        caller,
                        key: key.clone(),
                        request: Box::new(request.clone()),
                        review: review.clone(),
                    },
                )
            } else {
                Err(error)
            }
        }
    }
}

/// Exact saved acknowledgment, independent of current/superseding grants,
/// settings and engine. Missing/pruned evidence is unknown, never replay leave.
pub(crate) fn receipt(
    registry: &ServiceRegistry,
    caller: Incarnation,
    key: &RequestKey,
) -> Result<airk_core::service::ControlResponse> {
    let (job, expired) = registry.request_receipt(caller, key)?;
    let (configuration, result) = if let Some(job) = &job {
        let work = registry.configuration_work(job.id)?;
        if let Some(work) = &work {
            let selection = selected(&work.request)?;
            ensure!(
                selection.caller == caller && selection.key == *key,
                "saved refresh receipt belongs to a different intent"
            );
        }
        ensure!(
            job.kind == airk_core::service::JobKind::Configure,
            "saved key is not a refresh configuration job"
        );
        let result = if work.is_some() {
            registry.configuration_result(job.id)?
        } else {
            None
        };
        (work, result)
    } else {
        (None, None)
    };
    Ok(
        airk_core::service::ControlResponse::ReferenceRefreshReceipt {
            job,
            configuration,
            result,
            expired,
        },
    )
}

pub(crate) fn execute(
    kernel: &airk_kernel::Kernel,
    registry: &mut ServiceRegistry,
    lease: &ProjectLease,
    job: airk_core::identity::Sha256Digest,
) -> Result<JobOutcome> {
    let project = registry.job(job)?.project;
    let work = registry
        .configuration_work(job)?
        .context("accepted refresh work missing")?;
    ensure!(
        work.engine == crate::runtime_pair::engine_identity()?,
        "refresh engine changed; new review required"
    );
    let selection = selected(&work.request)?;
    // A queued operation rechecks current grant/context/backing before its
    // first effect; a changed grant does not silently expand permission.
    let before = selection.members[0].original_publication;
    let effect = (|| {
        qualify(registry, project, &work.request)?;
        kernel.refresh_service_references(project, &work.request)
    })();
    let report = match effect {
        Ok(report) => report,
        Err(error) => {
            if let Some(review) =
                error.downcast_ref::<airk_core::catalog_refresh::RefreshReviewRequired>()
            {
                review.validate()?;
                let refusal = serde_json::json!({"schema":"airk.reference-refresh-review-required/1","selection_digest":selection.identity()?,"original_query_digest":selection.original_query_digest,"review":review,"publication_applied":false,"requery_executed":false});
                registry.record_configuration_result(lease, job, &refusal)?;
                return Ok(JobOutcome::Failed {
                    code: "refresh_scope_changed_review_required".into(),
                });
            } else {
                return Err(error);
            }
        }
    };
    let updated = kernel.refresh_service_snapshot()?;
    let after = updated.status_summary()?;
    let publication: airk_core::identity::RevisionToken =
        serde_json::from_value(after["publication"]["token"].clone())?;
    // Complete native output is saved or the job remains failed/unknown. Never
    // truncate to fit the result slot or attach new hashes to original snippets.
    let result = serde_json::json!({"schema":"airk.reference-refresh-result/1",
        "selection_digest":selection.identity()?,"original_query_digest":selection.original_query_digest,
        "effective_mode":selection.mode,"effective_scope":if selection.mode==RefreshMode::SelectedOnly {"selected_sources"} else {"whole_index_explicitly_reviewed"},
        "before_publication":before,"after_publication":publication,
        "report":report,"status":after,"requery_executed":false});
    airk_core::index_catalog::encoded_size(
        &result,
        airk_core::service_configuration::MAX_CONFIGURATION_BYTES,
    )?;
    registry.record_configuration_result(lease, job, &result)?;
    let state = after["semantic_readiness"]["state"].as_str();
    registry.set_background_pending(
        lease,
        airk_core::service::JobKind::Vectors,
        matches!(state, Some("pending" | "partial")),
    )?;
    Ok(JobOutcome::Succeeded {
        revision: Some(publication),
    })
}
