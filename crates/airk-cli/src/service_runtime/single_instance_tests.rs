// SPDX-License-Identifier: Apache-2.0
use super::*;
use anyhow::Context;

#[test]
fn a_second_service_start_is_refused_typed_while_the_lease_is_held() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("control");
    std::fs::create_dir(&root)?;
    let running = acquire_single_instance(&root)?;
    let refused = acquire_single_instance(&root)
        .err()
        .context("a second service must not acquire the held lease")?;
    let message = format!("{refused:#}");
    assert!(message.starts_with("service_already_running:"), "{message}");
    // Releasing the one owner lets the next start take the same lease.
    drop(running);
    acquire_single_instance(&root)?;
    Ok(())
}
