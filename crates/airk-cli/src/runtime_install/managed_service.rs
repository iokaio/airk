// SPDX-License-Identifier: Apache-2.0
//! Direct per-user Windows/Linux manager entry; never forwards or launches a child.
use super::*;

pub(super) async fn serve(root: &Path) -> Result<()> {
    ensure!(
        cfg!(any(windows, target_os = "linux")),
        "direct task/systemd entry is unavailable on this platform"
    );
    managed::require_unmanaged(root)?;
    // Startup may be requested while the installer holds its mutation lease.
    // Authenticate immutable captured metadata without reacquiring that lease;
    // the service loop acquires its independent single-instance ownership.
    let (selected, approval) = startup_selected(root)?;
    let current = std::env::current_exe()?.canonicalize()?;
    ensure!(
        current == selected.canonicalize()?,
        "direct manager must execute the selected immutable main in place"
    );
    let manager = crate::service_manager::ServiceManager::for_runtime(root)?;
    manager.require_direct_entry(&current, &approval.manifest_sha256)?;
    let control = layout::control(root)?;
    service_runtime::serve(&control).await
}
