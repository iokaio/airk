// SPDX-License-Identifier: Apache-2.0
use super::current_inspection::InspectCurrent;
use airk_kernel::{Kernel, limits::ScanLimits};
use airk_store::FileStore;
use anyhow::Result;
use std::fs;

const HEADER: &str = "int original_beacon(void);\n";
const CONFIG: &str = r#"[{"file":"beacon.h","arguments":["unexecuted","-x","c++"]}]"#;
fn policy() -> airk_kernel::extraction::ExtractionPolicy {
    serde_json::from_value(serde_json::json!({"version":2,"rules":[{"selector":{"kind":"header_context","prefix":"","configuration":"compile_commands.json"},"case_sensitive":true,"adapter_id":"cpp"}]})).unwrap()
}

#[test]
fn missing_ignored_oversized_and_invalid_context_never_select_cpp() -> Result<()> {
    for mode in ["missing", "ignored", "oversized", "invalid"] {
        let root = tempfile::tempdir()?;
        let state = tempfile::tempdir()?;
        fs::write(root.path().join("beacon.h"), HEADER)?;
        match mode {
            "ignored" => {
                fs::write(root.path().join("compile_commands.json"), CONFIG)?;
                fs::write(root.path().join(".airkignore"), "compile_commands.json\n")?;
            }
            "oversized" => fs::write(
                root.path().join("compile_commands.json"),
                format!("{CONFIG}{}", " ".repeat(131_073)),
            )?,
            "invalid" => fs::write(root.path().join("compile_commands.json"), "[{broken")?,
            _ => {}
        }
        let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
        kernel.configure_extraction_policy(policy())?;
        let report = kernel.index()?;
        let store = FileStore::new(state.path().join("collections-1.2/default"));
        assert_eq!(
            store
                .current_test_catalog()?
                .unwrap()
                .inventory
                .as_ref()
                .unwrap()
                .files["beacon.h"]
                .language,
            "c",
            "{mode}"
        );
        assert!(
            report
                .selection_diagnostics
                .iter()
                .any(|warning| warning.path == "beacon.h")
        );
        assert_eq!(fs::read_to_string(root.path().join("beacon.h"))?, HEADER);
    }
    Ok(())
}

#[test]
fn captured_source_bytes_consume_scan_budget_without_double_charging() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(root.path().join("beacon.h"), HEADER)?;
    // More than the minimum configurable read budget, still below the context cap.
    let config = format!("{CONFIG}{}", " ".repeat(2048));
    fs::write(root.path().join("compile_commands.json"), &config)?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    kernel.index()?;
    let generation = kernel.status()?.generation;
    kernel.configure_extraction_policy(policy())?;
    let original = kernel.scan_limits()?;
    kernel.configure_scan_limits(ScanLimits {
        read_bytes: 1024,
        ..original
    })?;
    assert!(kernel.index().is_err());
    assert_eq!(kernel.status()?.generation, generation);
    kernel.configure_scan_limits(ScanLimits {
        read_bytes: config.len() + HEADER.len(),
        ..original
    })?;
    kernel.index()?;
    let store = FileStore::new(state.path().join("collections-1.2/default"));
    assert_eq!(
        store
            .current_test_catalog()?
            .unwrap()
            .inventory
            .as_ref()
            .unwrap()
            .files["beacon.h"]
            .language,
        "cpp"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("compile_commands.json"))?,
        config
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlinked_configuration_is_ineligible() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let external = tempfile::tempdir()?;
    fs::write(root.path().join("beacon.h"), HEADER)?;
    fs::write(external.path().join("commands.json"), CONFIG)?;
    std::os::unix::fs::symlink(
        external.path().join("commands.json"),
        root.path().join("compile_commands.json"),
    )?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    kernel.configure_extraction_policy(policy())?;
    kernel.index()?;
    let store = FileStore::new(state.path().join("collections-1.2/default"));
    assert_eq!(
        store
            .current_test_catalog()?
            .unwrap()
            .inventory
            .as_ref()
            .unwrap()
            .files["beacon.h"]
            .language,
        "c"
    );
    Ok(())
}
