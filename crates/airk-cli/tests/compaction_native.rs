// SPDX-License-Identifier: Apache-2.0
//! Real guarded compaction follows the selected incarnation after sibling rebuild.
mod common;
use airk_core::{
    service::{ControlMethod as Method, ControlResponse as Response, JobKind},
    service_maintenance::RepairOperation,
};
use anyhow::{Context, Result};
use std::fs;

#[test]
fn native_compaction_after_rebuild_reports_the_selected_publication_and_actual_bytes() -> Result<()>
{
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        source.path().join("original.rs"),
        "pub fn CompactNeedle() {}\n",
    )?;
    let service = common::Service::start(source.path(), data.path(), "compact-native");
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    service.job(JobKind::Index);
    let collection = data.path().join("collections-1.4/compact-native");
    let prior: serde_json::Value =
        serde_json::from_slice(&fs::read(collection.join("active.json"))?)?;
    service.repair(RepairOperation::Rebuild);
    let active: serde_json::Value =
        serde_json::from_slice(&fs::read(collection.join("active.json"))?)?;
    assert_ne!(prior["active"], active["active"]);
    let current = collection
        .join("incarnations")
        .join(active["active"].as_str().context("active slot missing")?)
        .join("current.airk");
    let status = common::call(
        source.path(),
        data.path(),
        "compact-native",
        &[
            "tools",
            "call",
            "airk_status",
            "--args",
            r#"{"response_mode":"full"}"#,
        ],
    )?;
    let before: serde_json::Value =
        serde_json::from_str(status["content"][0]["text"].as_str().unwrap())?;
    let bytes_before = fs::metadata(&current)?.len();
    let plan = service.repair_plan(RepairOperation::Compact);
    let Response::PlanItems { page } = service.request(Method::PlanItems {
        id: plan,
        after: None,
        limit: 32,
    }) else {
        anyhow::bail!("compact receipt unavailable")
    };
    let receipt = page.items[0]
        .compaction
        .as_ref()
        .context("successful compaction lost measured receipt")?;
    assert_eq!(
        serde_json::to_value(receipt.token)?,
        before["publication"]["token"]
    );
    assert_eq!(receipt.bytes_before, bytes_before);
    // A later native verification may allocate allocator pages without changing
    // the publication. The durable receipt describes the compaction boundary.
    assert!(receipt.bytes_after <= fs::metadata(&current)?.len());
    let sampled = common::call(
        source.path(),
        data.path(),
        "compact-native",
        &[
            "tools",
            "call",
            "airk_status",
            "--args",
            r#"{"response_mode":"full"}"#,
        ],
    )?;
    let sampled: serde_json::Value =
        serde_json::from_str(sampled["content"][0]["text"].as_str().unwrap())?;
    assert_eq!(
        sampled["storage_sizes"]["current_index_bytes"],
        fs::metadata(&current)?.len()
    );
    assert_eq!(
        sampled["publication"]["token"],
        before["publication"]["token"]
    );
    assert_eq!(
        receipt.bytes_reclaimed,
        receipt.bytes_before.saturating_sub(receipt.bytes_after)
    );
    let found = common::call(
        source.path(),
        data.path(),
        "compact-native",
        &[
            "tools",
            "call",
            "airk_search",
            "--args",
            r#"{"query":"CompactNeedle","mode":"lexical","response_mode":"full"}"#,
        ],
    )?;
    let found: serde_json::Value =
        serde_json::from_str(found["content"][0]["text"].as_str().unwrap())?;
    assert_eq!(found["hits"][0]["citation"]["path"], "original.rs");
    assert_eq!(
        fs::read_to_string(source.path().join("original.rs"))?,
        "pub fn CompactNeedle() {}\n"
    );
    Ok(())
}
