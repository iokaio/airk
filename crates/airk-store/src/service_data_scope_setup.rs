// SPDX-License-Identifier: Apache-2.0
//! Native setup encoding/validation only. Publication is descriptor-relative in
//! the owning setup adapter; these routines perform no filesystem mutation.
use super::*;

pub(crate) fn initial_owner_bytes(
    data: &Path,
    control: &Path,
    namespace: Incarnation,
) -> Result<Vec<u8>> {
    let scope = expected(data, control, namespace)?;
    ensure!(
        scope.data == data && scope.control == control,
        "native setup ownership labels must retain their canonical reviewed paths"
    );
    let bytes = serde_json::to_vec(&scope)?;
    ensure!(
        bytes.len() as u64 <= MAX_BYTES,
        "setup data ownership exceeds bound"
    );
    Ok(bytes)
}

#[cfg(any(unix, windows))]
pub(crate) fn bound_from_existing_setup(
    data: &Path,
    control: &Path,
    namespace: Incarnation,
    actual: &[u8],
) -> Result<BoundDataRoot> {
    ensure!(
        actual.len() as u64 <= MAX_BYTES,
        "setup data ownership exceeds bound"
    );
    let scope = expected(data, control, namespace)?;
    let saved: Scope = serde_json::from_slice(actual)?;
    ensure!(
        saved == scope && serde_json::to_vec(&saved)? == actual,
        "native setup data ownership changed"
    );
    Ok(BoundDataRoot(scope, LiveIntegrity(Default::default())))
}
