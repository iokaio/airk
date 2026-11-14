// SPDX-License-Identifier: Apache-2.0
//! Opaque original admission custody retained beyond local result construction.
use super::*;
use airk_catalog_control::catalog_admission::{IssueTicket, NativeCatalogAdmission};
use airk_core::{
    identity::{Sha256Digest, framed_digest},
    scoped_reads::SelectionScopeRef,
};
use airk_mcp::read_attempt::{ReadAttemptContext, ReadSourceDeliveryGuard, ReadSourceGuardKind};
use std::task::Poll;

struct SourceDelivery {
    gate: NativeCatalogAdmission,
    issue: IssueTicket,
    selection: Option<SelectionScopeRef>,
    identity: Sha256Digest,
}
impl ReadSourceDeliveryGuard for SourceDelivery {
    fn kind(&self) -> ReadSourceGuardKind {
        if self.selection.is_some() {
            ReadSourceGuardKind::Selection
        } else {
            ReadSourceGuardKind::Epoch
        }
    }
    fn identity(&self) -> Sha256Digest {
        self.identity
    }
    fn check(&self) -> std::io::Result<()> {
        self.gate.check_issue(self.issue).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "native original source issue revoked",
            )
        })
    }
    fn poll(
        &self,
        effect: &mut dyn FnMut() -> Poll<std::io::Result<usize>>,
    ) -> Poll<std::io::Result<usize>> {
        let result = match &self.selection {
            Some(selection) => self.gate.with_live_selection(self.issue, selection, effect),
            None => self.gate.with_issue(self.issue, effect),
        };
        result.unwrap_or_else(|_| {
            Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "native original source selection revoked",
            )))
        })
    }
}
pub(super) fn retain_epoch(state: &Arc<State>, context: &ReadAttemptContext) -> Result<()> {
    let gate = state
        .catalog_gate
        .as_ref()
        .context("native source admission unavailable")?;
    let clock = gate
        .catalog_clock()
        .context("native source clock unavailable")?;
    let issue = gate.begin_issue(clock.namespace, clock.revision)?;
    let identity = framed_digest(
        b"airk.native-read-delivery-epoch/1",
        &[&serde_json::to_vec(&(
            state.instance,
            context.request_id(),
        ))?],
    );
    context.retain_source_delivery_guard(Arc::new(SourceDelivery {
        gate: gate.clone(),
        issue,
        selection: None,
        identity,
    }))
}
pub(super) fn retain_selection(
    state: &Arc<State>,
    context: &ReadAttemptContext,
    selection: SelectionScopeRef,
) -> Result<()> {
    let gate = state
        .catalog_gate
        .as_ref()
        .context("native source admission unavailable")?;
    let clock = gate
        .catalog_clock()
        .context("native source clock unavailable")?;
    ensure!(
        selection.service_instance == state.instance,
        "foreign native source selection service"
    );
    let issue = gate.capture_current_issue(clock.namespace, selection.catalog_revision)?;
    gate.with_live_selection(issue, &selection, || ())?;
    let identity = framed_digest(
        b"airk.native-read-delivery-selection/1",
        &[&serde_json::to_vec(&selection)?],
    );
    context.retain_source_delivery_guard(Arc::new(SourceDelivery {
        gate: gate.clone(),
        issue,
        selection: Some(selection),
        identity,
    }))
}
#[cfg(test)]
mod tests;
