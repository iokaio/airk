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

/// Why a setup review refused its inputs, as a failure code (the service
/// answers `Failure { code }` with it instead of the generic
/// `invalid_registry_request`). Attached as context, so the original error
/// chain stays for diagnosis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexSetupRefused(pub &'static str);
impl std::fmt::Display for IndexSetupRefused {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output.write_str(self.0)
    }
}

/// Pure: the refusal code of a setup review error, when it is one the user
/// can act on (a missing folder, overlapping folders, occupied or redirected
/// storage); `None` keeps the generic code.
pub fn setup_refusal(error: &anyhow::Error) -> Option<&'static str> {
    if error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound)
    }) {
        return Some("index_setup_path_missing");
    }
    let text = format!("{error:#}");
    [
        // The most specific causes first: the first match wins.
        ("alias namespace", "index_setup_alias_unavailable"),
        ("must already exist", "index_setup_path_missing"),
        ("overlaps", "index_setup_paths_overlap"),
        ("must be disjoint", "index_setup_paths_overlap"),
        ("roots overlap", "index_setup_paths_overlap"),
        ("occupied", "index_setup_storage_occupied"),
        ("redirected", "index_setup_path_redirected"),
        ("linked", "index_setup_path_redirected"),
        ("must be a directory", "index_setup_path_not_a_folder"),
        ("plain directory", "index_setup_path_not_a_folder"),
        (
            "additional source is not a directory",
            "index_setup_path_not_a_folder",
        ),
    ]
    .into_iter()
    .find(|(needle, _)| text.contains(needle))
    .map(|(_, code)| code)
}

fn refused(error: anyhow::Error) -> anyhow::Error {
    match setup_refusal(&error) {
        Some(code) => error.context(IndexSetupRefused(code)),
        None => error,
    }
}

pub fn execute(
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
            let registration = airk_host::service_registration::prepare(
                Path::new(&selection.primary_root),
                Path::new(&selection.data_root),
                &selection.collection,
            )
            .map_err(refused)?;
            let mut sources = vec![SetupSource {
                canonical_path: registration.root.clone(),
                aliases: Vec::new(),
                owner: true,
            }];
            for root in &selection.additional_roots {
                let path = Path::new(&root.path)
                    .canonicalize()
                    .map_err(|error| refused(error.into()))?;
                if !path.is_dir() {
                    return Err(refused(anyhow::anyhow!(
                        "additional source is not a directory"
                    )));
                }
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
            let review = crate::catalog_index_setup::review(control, &request).map_err(refused)?;
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
                legacy_adoption: None,
                prepared_digest: prepared
                    .preparation
                    .as_ref()
                    .context("native preparation missing")?
                    .digest,
                expected_catalog_revision: state.revision,
                expected_source_revision: prepared.source_revision,
            };
            ControlResponse::CatalogIndexSetupApplied {
                // U103: the new index is readable through its own index set.
                receipt: registry.complete_index_setup_with_index_set(
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

#[cfg(test)]
mod refusal_tests {
    use super::*;

    #[test]
    fn setup_review_errors_name_what_the_user_can_fix() {
        let missing = anyhow::Error::from(std::io::Error::from(std::io::ErrorKind::NotFound))
            .context("pin custom storage parent");
        assert_eq!(setup_refusal(&missing), Some("index_setup_path_missing"));
        for (message, code) in [
            (
                "service sources and planned storage must be disjoint",
                "index_setup_paths_overlap",
            ),
            (
                "native setup source overlaps private storage",
                "index_setup_paths_overlap",
            ),
            (
                "index storage overlaps service control",
                "index_setup_paths_overlap",
            ),
            (
                "custom index storage is occupied without matching native ownership",
                "index_setup_storage_occupied",
            ),
            (
                "native setup destination redirected",
                "index_setup_path_redirected",
            ),
            (
                "service source must be a directory",
                "index_setup_path_not_a_folder",
            ),
            (
                "native source alias namespace is occupied or unavailable",
                "index_setup_alias_unavailable",
            ),
        ] {
            assert_eq!(
                setup_refusal(&anyhow::anyhow!(message)),
                Some(code),
                "{message}"
            );
        }
        assert_eq!(
            setup_refusal(&anyhow::anyhow!("native setup catalog changed")),
            None
        );
        let wrapped = refused(anyhow::anyhow!("native setup destination redirected"));
        assert_eq!(
            wrapped.downcast_ref::<IndexSetupRefused>(),
            Some(&IndexSetupRefused("index_setup_path_redirected"))
        );
        assert!(format!("{wrapped:#}").contains("destination redirected"));
    }
}
