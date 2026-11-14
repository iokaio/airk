// SPDX-License-Identifier: Apache-2.0
//! M19 `Lifecycle` (U85; split step 3b.1e-2): split-native stop and restart on
//! the service's existing lifecycle ledger (`airk-store`, unchanged format).
//!
//! - Accept: the request is recorded (Draining, saved explicit Stop set) with
//!   this instance and executable; the service requests its drain before
//!   attempting the reply. Failed delivery never cancels that accepted drain.
//! - Drain end: the record this instance accepted advances (Stop → Complete,
//!   Restart → Starting).
//! - Admitted start (the private `service-detached` role, holding the
//!   single-instance lease): a predecessor's pending record is completed, so a
//!   crash never blocks the next start; an explicit start then clears the
//!   saved Stop, an automatic one honors it.
//!
//! OS-manager ownership and forced stop join with login startup (Phase 7).
use airk_core::{
    identity::Incarnation,
    install_record::StartMode,
    service::RequestKey,
    service_lifecycle::{LifecycleAction, LifecyclePhase, LifecycleRecord, LifecycleRequest},
};
use airk_store::service::registry::ServiceRegistry;
use anyhow::Result;
use std::path::Path;

const REGISTRY: &str = "service-1.4.sqlite3";

fn namespace(root: &Path) -> Result<Incarnation> {
    ServiceRegistry::open_saved(root)?.incarnation()
}

/// Record a stop or restart of this instance (a replay returns its record).
pub(super) fn accept(
    root: &Path,
    instance: Incarnation,
    executable_sha256: &str,
    action: LifecycleAction,
    caller: Incarnation,
    key: RequestKey,
) -> Result<LifecycleRecord> {
    let request = LifecycleRequest {
        caller,
        key,
        action,
        expected_instance: instance,
        runtime: executable_sha256.parse()?,
        manager: false,
    };
    ServiceRegistry::with_existing_lifecycle(root, namespace(root)?, |ledger| {
        ledger.begin_lifecycle(&request)
    })
}

/// The next phase once this instance has drained.
fn drained(action: LifecycleAction) -> LifecyclePhase {
    match action {
        LifecycleAction::Stop => LifecyclePhase::Complete,
        LifecycleAction::Restart => LifecyclePhase::Starting,
    }
}

/// After the drain: advance the record this instance accepted.
pub(super) fn finish_drain(root: &Path, record: &LifecycleRecord) -> Result<()> {
    ServiceRegistry::with_existing_lifecycle(root, record.request.key.namespace, |ledger| {
        ledger.advance_lifecycle(record, drained(record.request.action))
    })
    .map(|_| ())
}

/// The phases that complete a pending record left by a predecessor.
fn completion(record: &LifecycleRecord) -> Vec<LifecyclePhase> {
    match (record.request.action, record.phase) {
        (LifecycleAction::Restart, LifecyclePhase::Draining) => {
            vec![LifecyclePhase::Starting, LifecyclePhase::Complete]
        }
        (_, LifecyclePhase::Draining | LifecyclePhase::Starting) => vec![LifecyclePhase::Complete],
        _ => Vec::new(),
    }
}

/// At an admitted start, holding the single-instance lease: complete a
/// predecessor's pending record, then clear the saved Stop for an explicit start.
pub(super) fn admit_start(root: &Path, mode: StartMode) -> Result<()> {
    if !root.join(REGISTRY).try_exists()? {
        return Ok(());
    }
    let saved = ServiceRegistry::open_saved(root)?;
    let namespace = saved.incarnation()?;
    let pending = saved.pending_lifecycle()?;
    drop(saved);
    ServiceRegistry::with_existing_lifecycle(root, namespace, |ledger| {
        if let Some(mut record) = pending {
            for next in completion(&record) {
                record = ledger.advance_lifecycle(&record, next)?;
            }
        }
        if mode == StartMode::Explicit {
            ledger.clear_service_stop()?;
        }
        Ok(())
    })
}

/// M20 (U80): abandon a stuck record. A terminal record answers unchanged,
/// as the CLI's abandon did; a record this instance is draining is refused,
/// because the drain settles it.
pub(super) fn abandon(
    root: &Path,
    instance: Incarnation,
    id: airk_core::identity::Sha256Digest,
) -> Result<LifecycleRecord> {
    let record = ServiceRegistry::open_saved(root)?
        .lifecycle_by_id(id)?
        .ok_or_else(|| anyhow::anyhow!("lifecycle_request_unknown"))?;
    if matches!(
        record.phase,
        LifecyclePhase::Complete | LifecyclePhase::Abandoned
    ) {
        return Ok(record);
    }
    if record.phase == LifecyclePhase::Draining && record.request.expected_instance == instance {
        anyhow::bail!("lifecycle_request_in_progress");
    }
    ServiceRegistry::with_existing_lifecycle(root, namespace(root)?, |ledger| {
        ledger.advance_lifecycle(&record, LifecyclePhase::Abandoned)
    })
}

/// The saved explicit Stop and the pending lifecycle record (U87).
pub(super) fn status(root: &Path) -> Result<(bool, Option<LifecycleRecord>)> {
    let saved = ServiceRegistry::open_saved(root)?;
    Ok((
        saved.service_explicitly_stopped()?,
        saved.pending_lifecycle()?,
    ))
}

/// The typed failure code for a refused lifecycle request: the ledger's own
/// snake-case refusal when it gave one, otherwise `lifecycle_unavailable`.
pub(super) fn failure_code(error: &anyhow::Error) -> String {
    let message = error.to_string();
    if !message.is_empty()
        && message.len() <= 64
        && message
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
    {
        message
    } else {
        "lifecycle_unavailable".into()
    }
}

#[cfg(test)]
mod tests;
