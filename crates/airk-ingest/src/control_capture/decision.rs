// SPDX-License-Identifier: Apache-2.0
//! Pure predicates used by the actual macOS syscall adapter and cross-host controls.
use anyhow::{Result, ensure};
pub(super) fn directory(uid: u32, mode: u32) -> Result<()> {
    ensure!(
        uid == 0 && mode & 0o170000 == 0o040000 && mode & 0o022 == 0,
        "system control directory is not root-protected"
    );
    Ok(())
}
pub(super) fn alias(uid: u32, mode: u32, links: u64, target: &[u8]) -> Result<()> {
    ensure!(
        uid == 0
            && mode & 0o170000 == 0o120000
            && mode & 0o022 == 0
            && links == 1
            && target == b"private/etc",
        "system control alias is unsupported"
    );
    Ok(())
}
pub(super) fn no_acl(query_status: i32, property_present: i32) -> Result<()> {
    ensure!(
        query_status == 0 && property_present == 0,
        "native control ACL is present or unknown"
    );
    Ok(())
}

/// One refused captured boundary cannot silently heal under the same scan owner.
pub(super) fn observe_once(
    failed: &std::sync::atomic::AtomicBool,
    observe: impl FnOnce() -> Result<()>,
) -> Result<()> {
    use std::sync::atomic::Ordering;
    ensure!(
        !failed.load(Ordering::Acquire),
        "system control anchor previously refused"
    );
    let result = observe();
    if result.is_err() {
        failed.store(true, Ordering::Release);
    }
    result
}
