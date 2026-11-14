// SPDX-License-Identifier: Apache-2.0
//! Transient expected-policy custody through the actual outer delivery poll.
use super::*;
use airk_core::{
    combined_context::ContextDeliveryBinding,
    identity::{Sha256Digest, framed_digest},
};
use airk_mcp::read_attempt::{ReadDeliveryGuard, ReadDeliveryGuardFactory};
use std::{collections::BTreeSet, task::Poll};

struct PolicyDelivery {
    configuration: Option<Arc<airk_store::service::arbiter::ConfigurationStore>>,
    expected: Vec<ContextDeliveryBinding>,
    digest: Sha256Digest,
    deadline: Instant,
    cancellation: tokio_util::sync::CancellationToken,
    service_live: Arc<dyn Fn() -> bool + Send + Sync>,
}
impl PolicyDelivery {
    fn live(&self) -> Result<()> {
        ensure!(
            !self.cancellation.is_cancelled() && Instant::now() < self.deadline,
            "original context delivery closed"
        );
        ensure!(
            (self.service_live)(),
            "native delivery service ended or draining"
        );
        Ok(())
    }
}
impl ReadDeliveryGuard for PolicyDelivery {
    fn bindings_digest(&self) -> Sha256Digest {
        self.digest
    }
    fn check_live(&self) -> std::io::Result<()> {
        self.live().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "original native policy delivery closed",
            )
        })
    }
    fn poll(
        &self,
        effect: &mut dyn FnMut() -> Poll<std::io::Result<usize>>,
    ) -> Poll<std::io::Result<usize>> {
        let refused = || {
            Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "context_delivery_policy_stale_nonreplayable",
            )))
        };
        if self.expected.is_empty() {
            return match self.live() {
                Ok(()) => effect(),
                Err(_) => refused(),
            };
        }
        let Some(configuration) = &self.configuration else {
            return refused();
        };
        configuration
            .with_current(
                || self.live(),
                |current| {
                    validate_policy(current, &self.expected)?;
                    self.live()?;
                    // Store is held before EVERY outer native/credential/source poll.
                    // No callback here may enter lifecycle/application/status.
                    Ok(effect())
                },
            )
            .unwrap_or_else(|_| refused())
    }
}
fn validate_policy(current: &Configuration, expected: &[ContextDeliveryBinding]) -> Result<()> {
    ensure!(
        !expected.is_empty() && expected.len() <= airk_core::index_catalog::MAX_SELECTION,
        "invalid context delivery roster"
    );
    let mut indexes = BTreeSet::new();
    for binding in expected {
        ensure!(
            indexes.insert(binding.scope.index_id)
                && binding.scope.installation == current.installation
                && binding.scope.catalog == current.catalog
                && binding.scope.configuration_revision == current.revision,
            "context delivery configuration changed"
        );
        ensure!(
            profile_for_index(current, binding.scope.index_id) == binding.profile,
            "context delivery profile changed"
        );
        if binding.profile.is_some() {
            ensure!(
                current.eligible(binding.scope.index_id, Task::ContextAction),
                "context delivery task no longer eligible"
            );
        }
    }
    Ok(())
}
pub(in crate::service_runtime) fn factory(
    state: &Arc<super::super::State>,
) -> ReadDeliveryGuardFactory {
    let weak = Arc::downgrade(state);
    Arc::new(move |expected, context| {
        ensure!(
            expected.len() <= airk_core::index_catalog::MAX_SELECTION,
            "invalid context delivery roster"
        );
        ensure!(
            expected.is_empty() || context.request_id().is_some(),
            "context delivery requires original native attempt"
        );
        let state = weak.upgrade().context("native delivery service ended")?;
        let configuration = state
            .arbiter
            .as_ref()
            .map(|owner| owner.configuration.clone());
        ensure!(
            expected.is_empty() || configuration.is_some(),
            "native Arbiter policy owner unavailable"
        );
        let digest = framed_digest(
            b"airk.context-delivery-bindings/1",
            &[&serde_json::to_vec(expected)?],
        );
        let live = weak.clone();
        Ok(Arc::new(PolicyDelivery {
            configuration,
            expected: expected.to_vec(),
            digest,
            deadline: context.deadline(),
            cancellation: context.cancellation().clone(),
            service_live: Arc::new(move || {
                live.upgrade()
                    .is_some_and(|state| !state.draining.load(std::sync::atomic::Ordering::Acquire))
            }),
        }) as Arc<dyn ReadDeliveryGuard>)
    })
}

#[cfg(test)]
mod tests;
