// SPDX-License-Identifier: Apache-2.0
//! The final user-facing reason for a failed download is fixed text with a retry
//! note; transport detail, hosts and signed query strings are never shown.
use super::{download::Failure, lifecycle::user_failure_reason};

const SECRET: &str = "https://example.invalid/part?X-Amz-Signature=secret-token";

#[test]
fn source_failure_after_all_attempts_says_try_again_later_without_transport_detail() {
    let error =
        anyhow::anyhow!("Arbiter asset source attempts stopped: source ordinal 0: source_failure")
            .context(Failure::Source(anyhow::anyhow!(SECRET)));
    let reason = user_failure_reason(&error);
    assert_eq!(
        reason,
        "The model download failed because the download source is unavailable or returned an unexpected response. Nothing was enabled. Try again later."
    );
    assert!(!reason.contains("secret-token") && !reason.contains("example.invalid"));
}

#[test]
fn deadline_failure_says_try_again_later() {
    let error = anyhow::anyhow!("stopped").context(Failure::Deadline);
    assert_eq!(
        user_failure_reason(&error),
        "The model download did not finish before its deadline. Nothing was enabled. Try again later."
    );
}

#[test]
fn local_custody_and_other_failures_keep_their_native_detail() {
    let local = anyhow::anyhow!("stopped").context(Failure::Local(anyhow::anyhow!("disk full")));
    let reason = user_failure_reason(&local);
    assert!(
        reason.contains("local_custody_or_policy_failure") && !reason.contains("Try again later")
    );
    let other = anyhow::anyhow!("Arbiter provisioning canceled");
    assert_eq!(user_failure_reason(&other), "Arbiter provisioning canceled");
}
