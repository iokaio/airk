// SPDX-License-Identifier: Apache-2.0
//! Typed hints for retrying the same read without substituting a revision.
use airk_store::{
    current::{CurrentGateContention, CurrentReadContention},
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
    let (code, retry_after_ms) = if error.downcast_ref::<SelectionContention>().is_some() {
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
            anyhow::anyhow!("current revision expired"),
            anyhow::anyhow!("corrupt index"),
            anyhow::anyhow!("service incarnation selection is busy; retry after 25 ms"),
            anyhow::anyhow!("write lock").context(CurrentGateContention::Readers),
        ] {
            assert!(read_retry_hint(&error).is_none());
        }
    }
}
