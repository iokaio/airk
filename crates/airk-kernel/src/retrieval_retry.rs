// SPDX-License-Identifier: Apache-2.0
//! Typed hints for retrying the same read without substituting a revision.
use airk_store::{
    current::{CurrentGateContention, CurrentReadContention},
    quota::QuotaContention,
    service::collection::SelectionContention,
};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct ReadRetryHint {
    pub status: &'static str,
    pub retryable: bool,
    pub code: &'static str,
    pub retry_after_ms: u32,
    pub action: &'static str,
}

pub fn read_retry_hint(error: &anyhow::Error) -> Option<ReadRetryHint> {
    let (code, retry_after_ms) = if error.downcast_ref::<SelectionContention>().is_some()
        || error.downcast_ref::<QuotaContention>().is_some()
    {
        // selection_busy is the existing safe READ-admission category: a
        // selected read cannot observe shared storage until its accounting
        // gate admits it. This does not retry ledger operations or mutations;
        // durable job diagnostics retain the distinct quota_ledger_busy code.
        ("selection_busy", SelectionContention::RETRY_AFTER_MS)
    } else if error.downcast_ref::<CurrentReadContention>().is_some()
        || matches!(
            error.downcast_ref::<CurrentGateContention>(),
            Some(CurrentGateContention::Applying)
        )
    {
        ("publication_busy", 25)
    } else {
        return None;
    };
    Some(ReadRetryHint {
        status: "retry",
        retryable: true,
        code,
        retry_after_ms,
        action: "repeat_same_request",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_typed_temporary_read_rejections_are_retryable() {
        for error in [
            anyhow::Error::new(SelectionContention),
            anyhow::Error::new(CurrentReadContention),
            anyhow::Error::new(std::fs::TryLockError::WouldBlock).context(QuotaContention),
            anyhow::anyhow!("native lock").context(CurrentGateContention::Applying),
        ] {
            let hint = read_retry_hint(&error.context("read request")).unwrap();
            assert_eq!(hint.action, "repeat_same_request");
            assert!(hint.retryable && hint.retry_after_ms > 0 && hint.retry_after_ms <= 1000);
        }
        for error in [
            anyhow::Error::new(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "busy publication",
            )),
            anyhow::anyhow!("shared storage accounting ledger is temporarily busy"),
            anyhow::anyhow!("quota_ledger_busy"),
            anyhow::Error::new(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "shared storage accounting ledger is temporarily busy",
            )),
            anyhow::anyhow!("current revision expired"),
            anyhow::anyhow!("corrupt index"),
            anyhow::anyhow!("service incarnation selection is busy; retry after 25 ms"),
            anyhow::anyhow!("write lock").context(CurrentGateContention::Readers),
        ] {
            assert!(read_retry_hint(&error).is_none());
        }
    }
}

#[cfg(test)]
#[test]
fn quota_read_admission_preserves_wrapped_type_without_reclassifying_text() {
    let error = anyhow::Error::new(std::fs::TryLockError::WouldBlock)
        .context(airk_store::quota::QuotaContention)
        .context("private ledger read context");
    let hint = read_retry_hint(&error).expect("typed quota read admission");
    assert_eq!(hint.code, "selection_busy");
    assert_eq!(hint.retry_after_ms, 25);
    assert_eq!(hint.action, "repeat_same_request");
    assert_eq!(
        serde_json::to_value(hint).unwrap(),
        serde_json::json!({
            "status":"retry", "retryable":true, "code":"selection_busy",
            "retry_after_ms":25, "action":"repeat_same_request"
        })
    );
    assert_eq!(
        airk_store::diagnostics::failure_code(&error).as_deref(),
        Some("quota_ledger_busy")
    );
    assert!(read_retry_hint(&anyhow::anyhow!("{error}")).is_none());
}
