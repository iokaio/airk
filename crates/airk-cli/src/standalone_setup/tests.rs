// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn standalone_uninstall_requires_explicit_native_completion() -> Result<()> {
    require_completed_uninstall(
        &serde_json::json!({"uninstalled":true,"project_data_preserved":true}),
    )?;
    for receipt in [
        serde_json::json!({"uninstalled":false,"deferred_runtimes":["owned-runtime"],"resume":"runtime uninstall","project_data_preserved":true}),
        serde_json::json!({"uninstalled":false,"deferred_launcher":true,"resume":"runtime uninstall","project_data_preserved":true}),
        serde_json::json!({"project_data_preserved":true}),
    ] {
        assert!(
            require_completed_uninstall(&receipt)
                .unwrap_err()
                .to_string()
                .contains("preserve the installer")
        );
    }
    Ok(())
}
