// SPDX-License-Identifier: Apache-2.0
//! Typed confirmation of destructive admin calls (contracts §4.3b, U98).
//!
//! What must be typed is the shared core rule
//! (`CatalogOperation::confirmation_target`, `plan_confirmation_text`), so
//! the admin pages and `airkctl` cannot disagree. The current display name
//! is read through the same canonical handlers as every other read; the
//! browser's text must equal it exactly before anything is sent. The native
//! digest check still decides whether the reviewed draft is current.
use super::Dispatch;
use crate::admin_contract::{AdminCode, Confirmation, MAX_CONFIRM_TEXT_BYTES};
use airk_core::{
    catalog_management::{CatalogEntity, CatalogItem, ConfirmationTarget, plan_confirmation_text},
    index_catalog::CatalogRevision,
    service::{ControlMethod, ControlResponse},
};

/// The current display name of a catalog entity at `revision`.
async fn entity_name(
    dispatch: &Dispatch,
    entity: CatalogEntity,
    revision: CatalogRevision,
) -> Result<String, AdminCode> {
    let response = dispatch(ControlMethod::CatalogDetail {
        entity: entity.clone(),
        expected_catalog_revision: Some(revision),
    })
    .await
    .map_err(|_| AdminCode::AdminInternal)?;
    // A stale revision or a retired target leaves nothing to confirm against.
    let ControlResponse::CatalogDetail { detail, .. } = response else {
        return Err(AdminCode::AdminConfirmationMismatch);
    };
    match (entity, detail.item) {
        (CatalogEntity::Project { id }, Some(CatalogItem::Project(project)))
            if project.id == id =>
        {
            Ok(project.name)
        }
        (CatalogEntity::Index { id }, Some(CatalogItem::Index(index))) if index.id == id => {
            Ok(index.name)
        }
        _ => Err(AdminCode::AdminConfirmationMismatch),
    }
}

/// The text `rule` requires for `method`, or `None` when it needs none.
async fn expected(
    dispatch: &Dispatch,
    rule: Confirmation,
    method: &ControlMethod,
) -> Result<Option<String>, AdminCode> {
    match (rule, method) {
        (Confirmation::CatalogTarget, ControlMethod::CatalogApply { request, .. }) => {
            let revision = request.expected_catalog_revision;
            match request.operation.confirmation_target() {
                None => Ok(None),
                Some(ConfirmationTarget::Id(id)) => Ok(Some(id)),
                Some(ConfirmationTarget::ProjectName(id)) => {
                    entity_name(dispatch, CatalogEntity::Project { id }, revision)
                        .await
                        .map(Some)
                }
                Some(ConfirmationTarget::IndexName(id)) => {
                    entity_name(dispatch, CatalogEntity::Index { id }, revision)
                        .await
                        .map(Some)
                }
            }
        }
        (Confirmation::SplitOriginalIndex, ControlMethod::CatalogSplitApply { draft }) => {
            let request = &draft.request;
            entity_name(
                dispatch,
                CatalogEntity::Index {
                    id: request.original_index,
                },
                request.expected_catalog_revision,
            )
            .await
            .map(Some)
        }
        (Confirmation::PlanId, ControlMethod::PlanExecute { id, .. }) => {
            Ok(Some(plan_confirmation_text(id)))
        }
        // U100: the location's path as the service's inventory lists it now.
        (Confirmation::StorageTarget, ControlMethod::StoragePurge { kind, .. }) => {
            let response = dispatch(ControlMethod::StorageInventory {})
                .await
                .map_err(|_| AdminCode::AdminInternal)?;
            let ControlResponse::StorageInventory { inventory } = response else {
                return Err(AdminCode::AdminConfirmationMismatch);
            };
            inventory
                .locations
                .into_iter()
                .find(|location| location.kind == *kind)
                .map(|location| Some(location.path))
                .ok_or(AdminCode::AdminConfirmationMismatch)
        }
        _ => Err(AdminCode::AdminMalformedRequest),
    }
}

/// Refuse the call unless `text` equals the target's current name.
pub(super) async fn check(
    dispatch: &Dispatch,
    rule: Confirmation,
    method: &ControlMethod,
    text: Option<&str>,
) -> Result<(), AdminCode> {
    if text.is_some_and(|text| text.len() > MAX_CONFIRM_TEXT_BYTES) {
        return Err(AdminCode::AdminMalformedRequest);
    }
    match expected(dispatch, rule, method).await? {
        None => Ok(()),
        Some(name) if text == Some(name.as_str()) => Ok(()),
        Some(_) => Err(AdminCode::AdminConfirmationMismatch),
    }
}
