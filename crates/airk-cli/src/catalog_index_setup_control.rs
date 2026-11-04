// SPDX-License-Identifier: Apache-2.0
//! Explicit native setup on the existing accepted metadata actor. The saved
//! operation survives waiter cancellation; source files are never modified.
use airk_core::{
    catalog_setup::{IndexSetupDraft, IndexSetupRequest, SetupCompletion, SetupSource, SetupStage},
    service::{ControlMethod, ControlResponse},
};
use airk_store::service::registry::ServiceRegistry;
use anyhow::{Context, Result, ensure};
use std::path::Path;

pub(crate) fn execute(
    registry: &mut ServiceRegistry,
    control: &Path,
    method: &ControlMethod,
) -> Result<Option<ControlResponse>> {
    let ControlMethod::CatalogIndexSetupApply { draft } = method else {
        return execute_inner(registry, control, method);
    };
    draft.validate()?;
    ensure!(
        draft.request.caller == crate::local_ipc::caller_identity()?,
        "foreign setup caller"
    );
    let request_digest = draft.request.identity()?;
    match execute_inner(registry, control, method) {
        Ok(response) => Ok(response),
        Err(_) => {
            // A failed lookup/commit can hide an already accepted operation.
            // Never report generic known-no-write after an Apply attempt. The
            // original key survives so an explicit receipt read can reconcile.
            let observation = registry
                .index_setup_receipt(draft.request.caller, &draft.request.key)
                .ok()
                .flatten()
                .filter(|receipt| {
                    receipt.request == draft.request && receipt.request_digest == request_digest
                })
                .map(
                    |receipt| airk_core::service::CatalogOutcomeObservation::IndexSetup {
                        receipt: Box::new(receipt),
                    },
                );
            Ok(Some(ControlResponse::CatalogOutcomeUnknown {
                code: "catalog_index_setup_outcome_unknown".into(),
                caller: draft.request.caller,
                key: draft.request.key.clone(),
                request_digest,
                observation,
            }))
        }
    }
}

fn execute_inner(
    registry: &mut ServiceRegistry,
    control: &Path,
    method: &ControlMethod,
) -> Result<Option<ControlResponse>> {
    let caller = match method {
        ControlMethod::CatalogIndexSetupDraft { .. }
        | ControlMethod::CatalogIndexSetupApply { .. }
        | ControlMethod::CatalogIndexSetupReceipt { .. } => crate::local_ipc::caller_identity()?,
        _ => return Ok(None),
    };
    Ok(Some(match method {
        ControlMethod::CatalogIndexSetupDraft { selection } => {
            selection.validate()?;
            let registration = crate::service_registration::prepare(
                Path::new(&selection.primary_root),
                Path::new(&selection.data_root),
                &selection.collection,
            )?;
            let mut sources = vec![SetupSource {
                canonical_path: registration.root.clone(),
                aliases: Vec::new(),
                owner: true,
            }];
            for root in &selection.additional_roots {
                let path = Path::new(&root.path).canonicalize()?;
                ensure!(path.is_dir(), "additional source is not a directory");
                sources.push(SetupSource {
                    canonical_path: path
                        .to_str()
                        .context("source path is not UTF-8")?
                        .to_owned(),
                    aliases: vec![root.alias.clone()],
                    owner: false,
                });
            }
            let request = IndexSetupRequest {
                caller,
                key: selection.key.clone(),
                expected_catalog_revision: selection.expected_catalog_revision,
                name: selection.name.clone(),
                registration,
                sources,
                setup_watch: selection.setup_watch,
            };
            let state = registry
                .catalog_state()?
                .context("Indexing catalog is unavailable")?;
            ensure!(
                state.namespace == request.key.namespace
                    && state.revision == request.expected_catalog_revision,
                "setup review catalog changed"
            );
            let review = crate::catalog_index_setup::review(control, &request)?;
            ControlResponse::CatalogIndexSetupDraft {
                request_digest: request.identity()?,
                draft: IndexSetupDraft::new(
                    request,
                    review.storage_action(),
                    review.review_digest()?,
                )?,
            }
        }
        ControlMethod::CatalogIndexSetupApply { draft } => {
            draft.validate()?;
            ensure!(draft.request.caller == caller, "foreign setup caller");
            let request = &draft.request;
            let saved = registry.index_setup_receipt(caller, &request.key)?;
            if let Some(saved) = &saved {
                ensure!(
                    saved.request == *request && saved.request_digest == request.identity()?,
                    "setup request key conflict"
                );
                if saved.stage == SetupStage::Ready {
                    return Ok(Some(ControlResponse::CatalogIndexSetupApplied {
                        receipt: saved.clone(),
                    }));
                }
            }
            let review = crate::catalog_index_setup::review(control, request)?;
            if saved.is_none() {
                ensure!(
                    review.review_digest()? == draft.review_digest
                        && review.storage_action() == draft.storage_action,
                    "setup filesystem review changed; review again before applying"
                );
                registry.reserve_index_setup(request)?;
            }
            // Exact accepted retries revalidate owned partial state. Their old
            // create-leaf preview need not describe the directory now created
            // by that same operation; a different saved request never resumes.
            let prepared =
                crate::catalog_index_setup::prepare(registry, control, review)?.record(registry)?;
            let state = registry
                .catalog_state()?
                .context("setup catalog disappeared")?;
            let completion = SetupCompletion {
                prepared_digest: prepared
                    .preparation
                    .as_ref()
                    .context("native preparation missing")?
                    .digest,
                expected_catalog_revision: state.revision,
                expected_source_revision: prepared.source_revision,
            };
            ControlResponse::CatalogIndexSetupApplied {
                receipt: registry.complete_index_setup(
                    caller,
                    &request.key,
                    request.identity()?,
                    &completion,
                )?,
            }
        }
        ControlMethod::CatalogIndexSetupReceipt {
            caller: requested,
            key,
        } => {
            ensure!(*requested == caller, "foreign setup caller");
            ControlResponse::CatalogIndexSetupReceipt {
                receipt: registry.index_setup_receipt(caller, key)?,
            }
        }
        _ => unreachable!("setup method was classified before native work"),
    }))
}
