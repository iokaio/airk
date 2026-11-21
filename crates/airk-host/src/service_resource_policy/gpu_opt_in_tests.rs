// SPDX-License-Identifier: Apache-2.0
//! Real stored files for the decision "Service-wide GPU use is opt-in" in the
//! Core `arbiter/CONTRACT.md`. The stored bytes are written out by hand.
use super::*;
use airk_core::service_resources::GpuOptIn;
use std::path::PathBuf;

const FILE: &str = "resources-1.4.json";
const OFF: &[u8] = br#"{"allow_gpu":false,"query_concurrency":16,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#;
const EARLIER_ENABLED: &[u8] = br#"{"allow_gpu":true,"query_concurrency":16,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#;
const RESET: &[u8] = br#"{"allow_gpu":false,"gpu_opt_in":"default_reset","query_concurrency":16,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#;
const CHOSEN: &[u8] = br#"{"allow_gpu":true,"gpu_opt_in":"user_choice","query_concurrency":16,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#;

fn runtime(directory: &tempfile::TempDir, name: &str) -> Result<PathBuf> {
    let root = directory.path().join(name);
    crate::local_ipc::prepare_root(&root)?;
    Ok(root)
}

fn stored(root: &Path) -> Result<Vec<u8>> {
    Ok(std::fs::read(root.join(FILE))?)
}

#[test]
fn a_missing_file_and_a_file_without_the_field_are_off_without_a_notice() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = runtime(&directory, "runtime")?;
    let new = load(&root)?;
    assert!(!new.allow_gpu);
    assert_eq!(new.gpu_opt_in, None);
    assert!(!root.join(FILE).exists(), "reading created the file");

    let without_field: &[u8] = br#"{"query_concurrency":16,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#;
    std::fs::write(root.join(FILE), without_field)?;
    let loaded = load(&root)?;
    assert!(!loaded.allow_gpu);
    assert_eq!(loaded.gpu_opt_in, None);
    assert_eq!(stored(&root)?, without_field);
    Ok(())
}

#[test]
fn an_enabled_file_from_the_earlier_default_reads_as_off_with_the_notice() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = runtime(&directory, "runtime")?;
    std::fs::write(root.join(FILE), EARLIER_ENABLED)?;
    // Every start before the first accepted change reads the same state, and
    // reading rewrites nothing.
    for _ in 0..2 {
        let loaded = load(&root)?;
        assert!(!loaded.allow_gpu);
        assert_eq!(loaded.gpu_opt_in, Some(GpuOptIn::DefaultReset));
        assert_eq!(stored(&root)?, EARLIER_ENABLED);
    }
    // A change that leaves GPU use off stores the pending notice.
    save(&root, &load(&root)?)?;
    assert_eq!(stored(&root)?, RESET);
    let loaded = load(&root)?;
    assert!(!loaded.allow_gpu);
    assert_eq!(loaded.gpu_opt_in, Some(GpuOptIn::DefaultReset));
    Ok(())
}

#[test]
fn the_users_choice_is_stored_with_its_record_and_survives_reloads() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = runtime(&directory, "runtime")?;
    std::fs::write(root.join(FILE), EARLIER_ENABLED)?;
    save(&root, &load(&root)?.with_gpu_choice(true))?;
    assert_eq!(stored(&root)?, CHOSEN);
    for _ in 0..2 {
        let loaded = load(&root)?;
        assert!(loaded.allow_gpu, "the user's choice was reset again");
        assert_eq!(loaded.gpu_opt_in, Some(GpuOptIn::UserChoice));
        assert_eq!(stored(&root)?, CHOSEN);
    }
    // Turning it off leaves the earlier off bytes and no notice.
    save(&root, &load(&root)?.with_gpu_choice(false))?;
    assert_eq!(stored(&root)?, OFF);
    let loaded = load(&root)?;
    assert!(!loaded.allow_gpu);
    assert_eq!(loaded.gpu_opt_in, None);
    Ok(())
}

#[test]
fn answering_the_notice_with_off_clears_it() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = runtime(&directory, "runtime")?;
    std::fs::write(root.join(FILE), EARLIER_ENABLED)?;
    save(&root, &load(&root)?.with_gpu_choice(false))?;
    assert_eq!(stored(&root)?, OFF);
    let loaded = load(&root)?;
    assert!(!loaded.allow_gpu);
    assert_eq!(loaded.gpu_opt_in, None);
    Ok(())
}

#[test]
fn a_stored_off_value_stays_off_without_a_notice() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = runtime(&directory, "runtime")?;
    std::fs::write(root.join(FILE), OFF)?;
    let loaded = load(&root)?;
    assert!(!loaded.allow_gpu);
    assert_eq!(loaded.gpu_opt_in, None);
    assert_eq!(stored(&root)?, OFF);
    Ok(())
}

#[test]
fn a_carrier_copies_the_stored_bytes_and_the_successor_applies_the_rule() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let old = runtime(&directory, "old")?;
    let new = runtime(&directory, "new")?;
    std::fs::write(old.join(FILE), EARLIER_ENABLED)?;
    let carried = load_stored(&old)?;
    assert!(carried.allow_gpu);
    assert_eq!(carried.gpu_opt_in, None);
    save(&new, &carried)?;
    assert_eq!(stored(&new)?, EARLIER_ENABLED);
    assert_eq!(stored(&old)?, EARLIER_ENABLED);
    let loaded = load(&new)?;
    assert!(!loaded.allow_gpu);
    assert_eq!(loaded.gpu_opt_in, Some(GpuOptIn::DefaultReset));

    std::fs::write(old.join(FILE), CHOSEN)?;
    save(&new, &load_stored(&old)?)?;
    assert_eq!(stored(&new)?, CHOSEN);
    let loaded = load(&new)?;
    assert!(loaded.allow_gpu);
    assert_eq!(loaded.gpu_opt_in, Some(GpuOptIn::UserChoice));
    Ok(())
}

#[test]
fn contradictory_or_unknown_stored_records_are_configuration_errors() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = runtime(&directory, "runtime")?;
    let refused: [&[u8]; 3] = [
        br#"{"allow_gpu":true,"gpu_opt_in":"default_reset","query_concurrency":16,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#,
        br#"{"allow_gpu":false,"gpu_opt_in":"user_choice","query_concurrency":16,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#,
        br#"{"allow_gpu":true,"gpu_opt_in":"enabled","query_concurrency":16,"preset":"balanced","extraction_threads":null,"inference_threads":null,"memory_bytes":null,"background_io_bytes_per_second":null}"#,
    ];
    for bytes in refused {
        std::fs::write(root.join(FILE), bytes)?;
        assert!(load(&root).is_err());
        assert!(load_stored(&root).is_err());
        assert_eq!(stored(&root)?, bytes);
    }
    std::fs::write(root.join(FILE), CHOSEN)?;
    for contradictory in [
        ResourceConfiguration {
            allow_gpu: true,
            gpu_opt_in: Some(GpuOptIn::DefaultReset),
            ..Default::default()
        },
        ResourceConfiguration {
            allow_gpu: false,
            gpu_opt_in: Some(GpuOptIn::UserChoice),
            ..Default::default()
        },
    ] {
        assert!(save(&root, &contradictory).is_err());
        assert_eq!(stored(&root)?, CHOSEN);
    }
    Ok(())
}
