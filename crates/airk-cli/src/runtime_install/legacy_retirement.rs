// SPDX-License-Identifier: Apache-2.0
//! Explicit local runtime administration; never an evidence or MCP write route.
use super::*;
use airk_core::{
    catalog_upgrade_recovery::{
        UpgradeRetirementPreview, UpgradeRetirementReply, UpgradeRetirementRequest,
    },
    identity::Sha256Digest,
    index_catalog::{IndexId, SourceImportState},
};
use airk_store::service::registry::catalog::upgrade::{self, CatalogUpgrade};
use std::io::Read;

fn input<T: serde::de::DeserializeOwned>() -> Result<T> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(airk_core::service::MAX_FRAME_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= airk_core::service::MAX_FRAME_BYTES,
        "legacy retirement input exceeds bound"
    );
    Ok(serde_json::from_slice(&bytes)?)
}
fn installer(root: &Path) -> Result<(PathBuf, Sha256Digest)> {
    let identity = runtime_pair::engine_identity()?;
    let control = layout::catalog_control(root, identity)?;
    Ok((control, identity))
}
fn observation(
    root: &Path,
    request: &UpgradeRetirementRequest,
) -> Result<Option<UpgradeRetirementReply>> {
    request.validate()?;
    ensure!(
        request.caller == crate::local_ipc::caller_identity()?,
        "legacy retirement caller differs from authenticated native caller"
    );
    let control = layout::saved_control(root)?;
    let saved =
        upgrade::pending(&control)?.context("retained legacy upgrade journal unavailable")?;
    ensure!(
        saved.plan.id == request.upgrade_id
            && saved.plan_digest == request.plan_digest
            && saved.plan.installer_identity == request.installer_identity,
        "retirement plan differs"
    );
    layout::validate_catalog_control(&control, saved.plan.namespace)?;
    let reply = upgrade::recovery::legacy_retirement_observation(&control, request)?;
    if let Some(reply) = &reply {
        reply.validate()?;
    }
    Ok(reply)
}
pub(super) fn candidates(root: &Path, after: Option<IndexId>) -> Result<serde_json::Value> {
    let _lock = existing_install_lock(root)?;
    let control = layout::saved_control(root)?;
    let saved = upgrade::pending(&control)?.context("no pending legacy upgrade")?;
    layout::validate_catalog_control(&control, saved.plan.namespace)?;
    let mut eligible = saved
        .plan
        .indexes
        .iter()
        .filter(|index| index.source_import == SourceImportState::PendingMetadata)
        .collect::<Vec<_>>();
    eligible.sort_by_key(|index| index.index_id);
    let start = if let Some(after) = after {
        eligible
            .iter()
            .position(|index| index.index_id == after)
            .context("legacy retirement continuation differs from saved candidates")?
            + 1
    } else {
        0
    };
    let mut items = Vec::new();
    let page = |items: &Vec<serde_json::Value>| {
        let continuation = if start + items.len() < eligible.len() && !items.is_empty() {
            Some(eligible[start + items.len() - 1].index_id)
        } else {
            None
        };
        serde_json::json!({"schema":1,"upgrade_id":saved.plan.id,"plan_digest":saved.plan_digest,
            "installer_identity":saved.plan.installer_identity,"namespace":saved.plan.namespace,
            "after":after,"items":items,"continuation":continuation,
            "subset":after.is_some() || continuation.is_some(),"total_eligible":eligible.len()})
    };
    for index in eligible.iter().skip(start).take(16) {
        let authorization = saved.retirement_recovery.as_ref().and_then(|journal| {
            journal
                .authorizations
                .iter()
                .find(|authorization| authorization.preview.request.index_id == index.index_id)
        });
        let observed = if let Some(authorization) = authorization {
            Some(
                observation(root, &authorization.preview.request)?
                    .context("saved retirement observation unavailable")?,
            )
        } else {
            None
        };
        items.push(
            serde_json::json!({"index_id":index.index_id,"registration":index.registration,
            "source_import":index.source_import,"observation":observed}),
        );
        if serde_json::to_vec(&page(&items))?.len() > airk_core::service::MAX_FRAME_BYTES {
            items.pop();
            ensure!(
                !items.is_empty(),
                "one legacy retirement candidate exceeds reply budget"
            );
            break;
        }
    }
    let value = page(&items);
    ensure!(
        serde_json::to_vec(&value)?.len() <= airk_core::service::MAX_FRAME_BYTES,
        "legacy candidate list exceeds reply budget"
    );
    Ok(value)
}
pub(super) fn review(root: &Path, index: IndexId) -> Result<UpgradeRetirementReply> {
    let _lock = existing_install_lock(root)?;
    let (control, identity) = installer(root)?;
    let owner = CatalogUpgrade::resume_for_installer(&control, identity)?;
    let request = owner.prepare_legacy_retirement_request(
        index,
        crate::local_ipc::caller_identity()?,
        airk_kernel::service_broker::new_instance_id()?,
    )?;
    let preview = owner.preview_legacy_retirement(&request)?;
    let reply = UpgradeRetirementReply::Review {
        review_digest: preview.identity()?,
        preview,
    };
    reply.validate()?;
    Ok(reply)
}
pub(super) fn authorize(root: &Path) -> Result<UpgradeRetirementReply> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Reviewed {
        preview: UpgradeRetirementPreview,
        review_digest: Sha256Digest,
    }
    let reviewed: Reviewed = input()?;
    reviewed.preview.validate()?;
    ensure!(
        reviewed.preview.identity()? == reviewed.review_digest,
        "legacy review changed"
    );
    let request = reviewed.preview.request;
    ensure!(
        request.caller == crate::local_ipc::caller_identity()?,
        "legacy retirement caller differs from authenticated native caller"
    );
    let _lock = existing_install_lock(root)?;
    let (control, identity) = installer(root)?;
    ensure!(
        identity == request.installer_identity,
        "crossed retirement candidate"
    );
    activation::legacy_retirement_candidate(root, &control, identity)?;
    let owner = CatalogUpgrade::resume_for_installer(&control, identity)?;
    let authorization = owner.authorize_legacy_retirement(&request, reviewed.review_digest)?;
    let reply = UpgradeRetirementReply::Authorized { authorization };
    reply.validate()?;
    Ok(reply) // No activation here. A separately confirmed Resume releases/reacquires owners.
}
pub(super) fn receipt(root: &Path) -> Result<serde_json::Value> {
    let request: UpgradeRetirementRequest = input()?;
    // Exact durable lookup only: no owner acquisition, source/data open, resume or replay.
    Ok(serde_json::json!({"schema":1,"observation":observation(root, &request)?}))
}
pub(super) async fn resume(root: &Path) -> Result<UpgradeRetirementReply> {
    let request: UpgradeRetirementRequest = input()?;
    ensure!(
        runtime_pair::engine_identity()? == request.installer_identity,
        "legacy retirement resume requires the originally admitted candidate"
    );
    match observation(root, &request)? {
        Some(UpgradeRetirementReply::Applied { .. }) => (),
        Some(UpgradeRetirementReply::Authorized { .. }) => (),
        _ => bail!("no exact accepted retirement to resume; authorization is never replayed"),
    }
    let candidate = {
        let _lock = existing_install_lock(root)?;
        let Some(pending) = activation::pending(root)? else {
            return observation(root, &request)?.context("accepted retirement outcome unavailable");
        };
        let _ = pending;
        let control = layout::saved_control(root)?;
        activation::legacy_retirement_candidate(root, &control, request.installer_identity)?
    };
    activate(root, &candidate).await?;
    observation(root, &request)?.context("accepted retirement outcome remains unavailable")
}
